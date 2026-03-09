//! XML namespace prefix stripping for RFC 4575 conference-info documents.
//!
//! quick-xml's serde deserializer matches element names literally, including
//! any namespace prefix. Since producers use varying prefixes (`confInfo:`,
//! `ci:`, or the default namespace), we normalize by stripping all prefixes
//! before deserialization.

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};

use super::error::ConferenceInfoError;

/// Strip XML namespace prefixes from element names and remove xmlns
/// declarations, producing prefix-free XML suitable for serde deserialization.
pub(super) fn strip_namespace_prefixes(xml: &str) -> Result<String, ConferenceInfoError> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::new());

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let stripped = strip_start_element(e);
                writer
                    .write_event(Event::Start(stripped))
                    .map_err(xml_err)?;
            }
            Ok(Event::End(ref e)) => {
                let local = local_name_owned(e.name());
                writer
                    .write_event(Event::End(BytesEnd::new(local)))
                    .map_err(xml_err)?;
            }
            Ok(Event::Empty(ref e)) => {
                let stripped = strip_start_element(e);
                writer
                    .write_event(Event::Empty(stripped))
                    .map_err(xml_err)?;
            }
            Ok(Event::Eof) => break,
            Ok(other) => {
                writer
                    .write_event(other)
                    .map_err(xml_err)?;
            }
            Err(e) => return Err(xml_err(e)),
        }
    }

    String::from_utf8(writer.into_inner()).map_err(|e| ConferenceInfoError::Xml(e.to_string()))
}

/// Strip the namespace prefix from an element name and filter out xmlns attributes.
fn strip_start_element(e: &BytesStart<'_>) -> BytesStart<'static> {
    let local = local_name_owned(e.name());
    let mut stripped = BytesStart::new(local);

    for attr in e
        .attributes()
        .filter_map(Result::ok)
    {
        if is_xmlns_attr(&attr) {
            continue;
        }
        let key = String::from_utf8_lossy(
            attr.key
                .as_ref(),
        )
        .into_owned();
        let value = attr
            .unescape_value()
            .unwrap_or_default()
            .into_owned();
        stripped.push_attribute((key.as_str(), value.as_str()));
    }

    stripped
}

/// Extract the local name (after the colon) from a QName, returning an owned String.
fn local_name_owned(qname: quick_xml::name::QName<'_>) -> String {
    let full = String::from_utf8_lossy(qname.as_ref());
    match full.find(':') {
        Some(pos) => full[pos + 1..].to_owned(),
        None => full.into_owned(),
    }
}

/// Check if an attribute is an xmlns declaration (`xmlns` or `xmlns:*`).
fn is_xmlns_attr(attr: &Attribute<'_>) -> bool {
    let key = std::str::from_utf8(
        attr.key
            .as_ref(),
    )
    .unwrap_or("");
    key == "xmlns" || key.starts_with("xmlns:")
}

fn xml_err(e: impl std::fmt::Display) -> ConferenceInfoError {
    ConferenceInfoError::Xml(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_confinfo_prefix() {
        let input = r#"<confInfo:conference-info xmlns:confInfo="urn:ietf:params:xml:ns:conference-info" entity="sip:conf@example.com"><confInfo:users><confInfo:user entity="sip:alice@example.com"/></confInfo:users></confInfo:conference-info>"#;
        let output = strip_namespace_prefixes(input).unwrap();
        assert!(output.contains("<conference-info"));
        assert!(output.contains("<users>"));
        assert!(output.contains("<user "));
        assert!(!output.contains("confInfo:"));
        assert!(!output.contains("xmlns"));
    }

    #[test]
    fn preserves_unprefixed() {
        let input = r#"<conference-info entity="sip:conf@example.com"><users><user entity="sip:alice@example.com"/></users></conference-info>"#;
        let output = strip_namespace_prefixes(input).unwrap();
        assert!(output.contains(r#"<conference-info entity="sip:conf@example.com">"#));
        assert!(output.contains("<users>"));
    }

    #[test]
    fn strips_default_xmlns() {
        let input = r#"<conference-info xmlns="urn:ietf:params:xml:ns:conference-info" entity="sip:conf@example.com"><users/></conference-info>"#;
        let output = strip_namespace_prefixes(input).unwrap();
        assert!(!output.contains("xmlns"));
        assert!(output.contains(r#"entity="sip:conf@example.com"#));
    }

    #[test]
    fn strips_arbitrary_prefix() {
        let input = r#"<ci:conference-info xmlns:ci="urn:ietf:params:xml:ns:conference-info" entity="sip:x@y"><ci:conference-state><ci:user-count>3</ci:user-count></ci:conference-state></ci:conference-info>"#;
        let output = strip_namespace_prefixes(input).unwrap();
        assert!(!output.contains("ci:"));
        assert!(output.contains("<conference-state>"));
        assert!(output.contains("<user-count>"));
    }

    #[test]
    fn preserves_non_xmlns_attributes() {
        let input = r#"<confInfo:user xmlns:confInfo="urn:ietf:params:xml:ns:conference-info" entity="sip:alice@example.com" state="full"/>"#;
        let output = strip_namespace_prefixes(input).unwrap();
        assert!(output.contains(r#"entity="sip:alice@example.com""#));
        assert!(output.contains(r#"state="full""#));
    }
}
