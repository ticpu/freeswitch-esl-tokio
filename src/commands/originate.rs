//! Originate command builder with endpoint configuration, variable scoping,
//! and automatic quoting for socket application arguments.

use std::fmt;
use std::str::FromStr;

use indexmap::IndexMap;
use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use super::{originate_quote, originate_split, originate_unquote};

/// FreeSWITCH dialplan type for originate commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DialplanType {
    /// Inline dialplan: applications execute directly without XML lookup.
    Inline,
    /// XML dialplan: route through the XML dialplan engine.
    Xml,
}

impl fmt::Display for DialplanType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inline => f.write_str("inline"),
            Self::Xml => f.write_str("XML"),
        }
    }
}

impl FromStr for DialplanType {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "inline" => Ok(Self::Inline),
            "XML" => Ok(Self::Xml),
            _ => Err(OriginateError::ParseError(format!(
                "unknown dialplan type: {}",
                s
            ))),
        }
    }
}

/// Scope for channel variables in an originate command.
///
/// - `Enterprise` (`<>`) — applies across all threads (`:_:` separated)
/// - `Default` (`{}`) — applies to all channels in this originate
/// - `Channel` (`[]`) — applies only to one specific channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariablesType {
    /// `<>` scope — applies across all `:_:` separated threads.
    Enterprise,
    /// `{}` scope — applies to all channels in this originate.
    Default,
    /// `[]` scope — applies to one specific channel.
    Channel,
}

impl VariablesType {
    fn delimiters(self) -> (char, char) {
        match self {
            Self::Enterprise => ('<', '>'),
            Self::Default => ('{', '}'),
            Self::Channel => ('[', ']'),
        }
    }
}

/// Ordered set of channel variables with FreeSWITCH escaping.
///
/// Values containing commas are escaped with `\,`, single quotes with `\'`,
/// and values with spaces are wrapped in single quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variables {
    /// Scope of these variables on the originate command.
    pub vars_type: VariablesType,
    inner: IndexMap<String, String>,
}

fn escape_value(value: &str) -> String {
    let escaped = value
        .replace('\'', "\\'")
        .replace(',', "\\,");
    if escaped.contains(' ') {
        format!("'{}'", escaped)
    } else {
        escaped
    }
}

fn unescape_value(value: &str) -> String {
    let s = value
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(value);
    s.replace("\\,", ",")
        .replace("\\'", "'")
}

impl Variables {
    /// Create an empty variable set with the given scope.
    pub fn new(vars_type: VariablesType) -> Self {
        Self {
            vars_type,
            inner: IndexMap::new(),
        }
    }

    /// Create from an existing ordered map.
    pub fn with_vars(vars_type: VariablesType, vars: IndexMap<String, String>) -> Self {
        Self {
            vars_type,
            inner: vars,
        }
    }

    /// Insert or overwrite a variable.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner
            .insert(key.into(), value.into());
    }

    /// Look up a variable by name.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner
            .get(key)
            .map(|s| s.as_str())
    }

    /// Whether the set contains no variables.
    pub fn is_empty(&self) -> bool {
        self.inner
            .is_empty()
    }

    /// Number of variables.
    pub fn len(&self) -> usize {
        self.inner
            .len()
    }

    /// Iterate over key-value pairs in insertion order.
    pub fn iter(&self) -> indexmap::map::Iter<'_, String, String> {
        self.inner
            .iter()
    }

    /// Mutable iterator over key-value pairs in insertion order.
    pub fn iter_mut(&mut self) -> indexmap::map::IterMut<'_, String, String> {
        self.inner
            .iter_mut()
    }

    /// Mutable iterator over values in insertion order.
    pub fn values_mut(&mut self) -> indexmap::map::ValuesMut<'_, String, String> {
        self.inner
            .values_mut()
    }
}

impl Serialize for Variables {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.vars_type == VariablesType::Default {
            self.inner
                .serialize(serializer)
        } else {
            use serde::ser::SerializeStruct;
            let mut s = serializer.serialize_struct("Variables", 2)?;
            s.serialize_field("scope", &self.vars_type)?;
            s.serialize_field("vars", &self.inner)?;
            s.end()
        }
    }
}

impl<'de> Deserialize<'de> for Variables {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum VariablesRepr {
            Scoped {
                scope: VariablesType,
                vars: IndexMap<String, String>,
            },
            Flat(IndexMap<String, String>),
        }

        match VariablesRepr::deserialize(deserializer)? {
            VariablesRepr::Scoped { scope, vars } => Ok(Self {
                vars_type: scope,
                inner: vars,
            }),
            VariablesRepr::Flat(map) => Ok(Self {
                vars_type: VariablesType::Default,
                inner: map,
            }),
        }
    }
}

impl fmt::Display for Variables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (open, close) = self
            .vars_type
            .delimiters();
        f.write_fmt(format_args!("{}", open))?;
        for (i, (key, value)) in self
            .inner
            .iter()
            .enumerate()
        {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}={}", key, escape_value(value))?;
        }
        f.write_fmt(format_args!("{}", close))
    }
}

impl FromStr for Variables {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.len() < 2 {
            return Err(OriginateError::ParseError(
                "variable block too short".into(),
            ));
        }

        let (vars_type, inner_str) = match (s.as_bytes()[0], s.as_bytes()[s.len() - 1]) {
            (b'{', b'}') => (VariablesType::Default, &s[1..s.len() - 1]),
            (b'<', b'>') => (VariablesType::Enterprise, &s[1..s.len() - 1]),
            (b'[', b']') => (VariablesType::Channel, &s[1..s.len() - 1]),
            _ => {
                return Err(OriginateError::ParseError(format!(
                    "unknown variable delimiters: {}",
                    s
                )));
            }
        };

        let mut inner = IndexMap::new();
        // Split on commas not preceded by backslash
        for part in split_unescaped_commas(inner_str) {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| {
                    OriginateError::ParseError(format!("missing = in variable: {}", part))
                })?;
            inner.insert(key.to_string(), unescape_value(value));
        }

        Ok(Self { vars_type, inner })
    }
}

/// Split on commas that are not preceded by a backslash.
fn split_unescaped_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();

    for i in 0..bytes.len() {
        if bytes[i] == b',' && !(i > 0 && bytes[i - 1] == b'\\') {
            parts.push(&s[start..i]);
            start = i + 1;
        }
    }
    parts.push(&s[start..]);
    parts
}

// Endpoint is now defined in endpoint.rs — re-exported via pub use below.
pub use super::endpoint::Endpoint;

/// A single dialplan application with optional arguments.
///
/// Formats differently depending on [`DialplanType`]:
/// - Inline: `name:args`
/// - XML: `&name(args)`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    /// Application name (e.g. `park`, `conference`, `socket`).
    pub name: String,
    /// Application arguments, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

impl Application {
    /// Create an application with optional arguments.
    pub fn new(name: impl Into<String>, args: Option<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            args: args.map(|a| a.into()),
        }
    }

    /// Format as inline (`name:args`) or XML (`&name(args)`) syntax.
    pub fn to_string_with_dialplan(&self, dialplan: &DialplanType) -> String {
        let args = self
            .args
            .as_deref()
            .unwrap_or("");
        match dialplan {
            DialplanType::Inline => format!("{}:{}", self.name, args),
            DialplanType::Xml => format!("&{}({})", self.name, args),
        }
    }
}

/// Ordered list of applications for an originate command.
///
/// Inline dialplan allows multiple comma-separated apps; XML dialplan allows exactly one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationList(pub Vec<Application>);

impl ApplicationList {
    /// Format the list for the given dialplan type. XML allows exactly one app.
    pub fn to_string_with_dialplan(
        &self,
        dialplan: &DialplanType,
    ) -> Result<String, OriginateError> {
        match dialplan {
            DialplanType::Inline => {
                let parts: Vec<String> = self
                    .0
                    .iter()
                    .map(|app| app.to_string_with_dialplan(dialplan))
                    .collect();
                Ok(parts.join(","))
            }
            DialplanType::Xml => {
                if self
                    .0
                    .len()
                    != 1
                {
                    return Err(OriginateError::TooManyApplications);
                }
                Ok(self.0[0].to_string_with_dialplan(dialplan))
            }
        }
    }
}

/// Originate command builder: `originate <endpoint> <app> [dialplan] [context] [cid_name] [cid_num] [timeout]`.
///
/// Application arguments containing spaces are automatically single-quoted.
/// Implements both `Display` (for wire format) and `FromStr` (for round-trip parsing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Originate {
    /// Dial target (sofia gateway, loopback, or raw URI).
    pub endpoint: Endpoint,
    /// Application(s) to execute on the originated channel.
    pub applications: ApplicationList,
    /// Dialplan engine. `None` defaults to XML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialplan: Option<DialplanType>,
    /// Dialplan context. `None` uses the profile's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Caller ID name for the originated leg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid_name: Option<String>,
    /// Caller ID number for the originated leg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid_num: Option<String>,
    /// Timeout in seconds. `None` uses FreeSWITCH default (60s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

impl fmt::Display for Originate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dialplan = self
            .dialplan
            .unwrap_or(DialplanType::Xml);
        let apps = self
            .applications
            .to_string_with_dialplan(&dialplan)
            .map_err(|_| fmt::Error)?;

        write!(f, "originate {} {}", self.endpoint, originate_quote(&apps))?;

        if let Some(ref dp) = self.dialplan {
            write!(f, " {}", dp)?;
        }
        if let Some(ref ctx) = self.context {
            write!(f, " {}", ctx)?;
        }
        if let Some(ref name) = self.cid_name {
            write!(f, " {}", name)?;
        }
        if let Some(ref num) = self.cid_num {
            write!(f, " {}", num)?;
        }
        if let Some(timeout) = self.timeout {
            write!(f, " {}", timeout)?;
        }
        Ok(())
    }
}

impl FromStr for Originate {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s
            .strip_prefix("originate")
            .unwrap_or(s)
            .trim();
        let mut args = originate_split(s, ' ')?;

        if args.is_empty() {
            return Err(OriginateError::ParseError("empty originate".into()));
        }

        let endpoint_str = args.remove(0);
        let endpoint: Endpoint = endpoint_str.parse()?;

        if args.is_empty() {
            return Err(OriginateError::ParseError(
                "missing application in originate".into(),
            ));
        }

        let app_str = originate_unquote(&args.remove(0));

        let dialplan = args
            .first()
            .and_then(|s| {
                s.parse::<DialplanType>()
                    .ok()
            });
        if dialplan.is_some() {
            args.remove(0);
        }

        let applications = super::parse_application_list(&app_str, dialplan.as_ref())?;

        let context = if !args.is_empty() {
            Some(args.remove(0))
        } else {
            None
        };
        let cid_name = if !args.is_empty() {
            Some(args.remove(0))
        } else {
            None
        };
        let cid_num = if !args.is_empty() {
            Some(args.remove(0))
        } else {
            None
        };
        let timeout = if !args.is_empty() {
            Some(
                args.remove(0)
                    .parse::<u32>()
                    .map_err(|e| OriginateError::ParseError(format!("invalid timeout: {}", e)))?,
            )
        } else {
            None
        };

        Ok(Self {
            endpoint,
            applications,
            dialplan,
            context,
            cid_name,
            cid_num,
            timeout,
        })
    }
}

/// Errors from originate command parsing or construction.
#[derive(Debug, thiserror::Error)]
pub enum OriginateError {
    /// A single-quoted token was never closed.
    #[error("unclosed quote at: {0}")]
    UnclosedQuote(String),
    /// XML dialplan only allows one application; multiple were given.
    #[error("too many applications for non-inline dialplan")]
    TooManyApplications,
    /// General parse failure with a description.
    #[error("parse error: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::endpoint::{LoopbackEndpoint, SofiaEndpoint, SofiaGateway};

    // --- Variables ---

    #[test]
    fn variables_standard_chars() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "this_value");
        let result = vars.to_string();
        assert!(result.contains("test_key"));
        assert!(result.contains("this_value"));
    }

    #[test]
    fn variables_comma_escaped() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "this,is,a,value");
        let result = vars.to_string();
        assert!(result.contains("\\,"));
    }

    #[test]
    fn variables_spaces_quoted() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "this is a value");
        let result = vars.to_string();
        assert_eq!(
            result
                .matches('\'')
                .count(),
            2
        );
    }

    #[test]
    fn variables_single_quote_escaped() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "let's_this_be_a_value");
        let result = vars.to_string();
        assert!(result.contains("\\'"));
    }

    #[test]
    fn variables_enterprise_delimiters() {
        let mut vars = Variables::new(VariablesType::Enterprise);
        vars.insert("k", "v");
        let result = vars.to_string();
        assert!(result.starts_with('<'));
        assert!(result.ends_with('>'));
    }

    #[test]
    fn variables_channel_delimiters() {
        let mut vars = Variables::new(VariablesType::Channel);
        vars.insert("k", "v");
        let result = vars.to_string();
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
    }

    #[test]
    fn variables_default_delimiters() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", "v");
        let result = vars.to_string();
        assert!(result.starts_with('{'));
        assert!(result.ends_with('}'));
    }

    #[test]
    fn variables_parse_round_trip() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("origination_caller_id_number", "9005551212");
        vars.insert("sip_h_Call-Info", "<url>;meta=123,<uri>");
        let s = vars.to_string();
        let parsed: Variables = s
            .parse()
            .unwrap();
        assert_eq!(
            parsed.get("origination_caller_id_number"),
            Some("9005551212")
        );
        assert_eq!(parsed.get("sip_h_Call-Info"), Some("<url>;meta=123,<uri>"));
    }

    // --- Endpoint ---

    #[test]
    fn endpoint_uri_only() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        assert_eq!(ep.to_string(), "sofia/internal/123@example.com");
    }

    #[test]
    fn endpoint_uri_with_variable() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "1");
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: Some(vars),
        });
        assert_eq!(
            ep.to_string(),
            "{one_variable=1}sofia/internal/123@example.com"
        );
    }

    #[test]
    fn endpoint_variable_with_quote() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "one'quote");
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: Some(vars),
        });
        assert_eq!(
            ep.to_string(),
            "{one_variable=one\\'quote}sofia/internal/123@example.com"
        );
    }

    #[test]
    fn loopback_endpoint_display() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "1");
        let ep = Endpoint::Loopback(LoopbackEndpoint {
            extension: "aUri".into(),
            context: "aContext".into(),
            variables: Some(vars),
        });
        assert_eq!(ep.to_string(), "{one_variable=1}loopback/aUri/aContext");
    }

    #[test]
    fn sofia_gateway_endpoint_display() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "1");
        let ep = Endpoint::SofiaGateway(SofiaGateway {
            destination: "aUri".into(),
            profile: None,
            gateway: "internal".into(),
            variables: Some(vars),
        });
        assert_eq!(
            ep.to_string(),
            "{one_variable=1}sofia/gateway/internal/aUri"
        );
    }

    // --- Application ---

    #[test]
    fn application_xml_format() {
        let app = Application::new("testApp", Some("testArg"));
        assert_eq!(
            app.to_string_with_dialplan(&DialplanType::Xml),
            "&testApp(testArg)"
        );
    }

    #[test]
    fn application_inline_format() {
        let app = Application::new("testApp", Some("testArg"));
        assert_eq!(
            app.to_string_with_dialplan(&DialplanType::Inline),
            "testApp:testArg"
        );
    }

    // --- ApplicationList ---

    #[test]
    fn application_list_single_xml() {
        let list = ApplicationList(vec![Application::new("testApp1", Some("testArg1"))]);
        assert_eq!(
            list.to_string_with_dialplan(&DialplanType::Xml)
                .unwrap(),
            "&testApp1(testArg1)"
        );
    }

    #[test]
    fn application_list_single_inline() {
        let list = ApplicationList(vec![Application::new("testApp1", Some("testArg1"))]);
        assert_eq!(
            list.to_string_with_dialplan(&DialplanType::Inline)
                .unwrap(),
            "testApp1:testArg1"
        );
    }

    #[test]
    fn application_list_empty_xml_errors() {
        let list = ApplicationList(vec![]);
        assert!(list
            .to_string_with_dialplan(&DialplanType::Xml)
            .is_err());
    }

    #[test]
    fn application_list_empty_inline() {
        let list = ApplicationList(vec![]);
        assert_eq!(
            list.to_string_with_dialplan(&DialplanType::Inline)
                .unwrap(),
            ""
        );
    }

    #[test]
    fn application_list_two_xml_errors() {
        let list = ApplicationList(vec![
            Application::new("testApp1", Some("testArg1")),
            Application::new("testApp2", Some("testArg2")),
        ]);
        assert!(list
            .to_string_with_dialplan(&DialplanType::Xml)
            .is_err());
    }

    #[test]
    fn application_list_two_inline() {
        let list = ApplicationList(vec![
            Application::new("testApp1", Some("testArg1")),
            Application::new("testApp2", Some("testArg2")),
        ]);
        assert_eq!(
            list.to_string_with_dialplan(&DialplanType::Inline)
                .unwrap(),
            "testApp1:testArg1,testApp2:testArg2"
        );
    }

    // --- Originate ---

    #[test]
    fn originate_xml_display() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let apps = ApplicationList(vec![Application::new("conference", Some("1"))]);
        let orig = Originate {
            endpoint: ep,
            applications: apps,
            dialplan: Some(DialplanType::Xml),
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
        };
        assert_eq!(
            orig.to_string(),
            "originate sofia/internal/123@example.com &conference(1) XML"
        );
    }

    #[test]
    fn originate_inline_display() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let apps = ApplicationList(vec![Application::new("conference", Some("1"))]);
        let orig = Originate {
            endpoint: ep,
            applications: apps,
            dialplan: Some(DialplanType::Inline),
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
        };
        assert_eq!(
            orig.to_string(),
            "originate sofia/internal/123@example.com conference:1 inline"
        );
    }

    #[test]
    fn originate_from_string_round_trip() {
        let input = "originate {test='variable with quote'}sofia/internal/test@example.com 123";
        let orig: Originate = input
            .parse()
            .unwrap();
        assert!(orig
            .endpoint
            .to_string()
            .contains("sofia/internal/test@example.com"));
    }

    #[test]
    fn originate_socket_app_quoted() {
        let ep = Endpoint::Loopback(LoopbackEndpoint {
            extension: "9199".into(),
            context: "test".into(),
            variables: None,
        });
        let apps = ApplicationList(vec![Application::new(
            "socket",
            Some("127.0.0.1:8040 async full"),
        )]);
        let orig = Originate {
            endpoint: ep,
            applications: apps,
            dialplan: None,
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
        };
        assert_eq!(
            orig.to_string(),
            "originate loopback/9199/test '&socket(127.0.0.1:8040 async full)'"
        );
    }

    #[test]
    fn originate_socket_round_trip() {
        let input = "originate loopback/9199/test '&socket(127.0.0.1:8040 async full)'";
        let parsed: Originate = input
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), input);
        assert_eq!(
            parsed
                .applications
                .0[0]
                .args
                .as_deref(),
            Some("127.0.0.1:8040 async full")
        );
    }

    #[test]
    fn originate_display_round_trip() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let apps = ApplicationList(vec![Application::new("conference", Some("1"))]);
        let orig = Originate {
            endpoint: ep,
            applications: apps,
            dialplan: Some(DialplanType::Xml),
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
        };
        let s = orig.to_string();
        let parsed: Originate = s
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    // --- DialplanType ---

    #[test]
    fn dialplan_type_display() {
        assert_eq!(DialplanType::Inline.to_string(), "inline");
        assert_eq!(DialplanType::Xml.to_string(), "XML");
    }

    #[test]
    fn dialplan_type_from_str() {
        assert_eq!(
            "inline"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Inline
        );
        assert_eq!(
            "XML"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Xml
        );
    }

    // --- Serde ---

    #[test]
    fn serde_dialplan_type_xml() {
        let json = serde_json::to_string(&DialplanType::Xml).unwrap();
        assert_eq!(json, "\"xml\"");
        let parsed: DialplanType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DialplanType::Xml);
    }

    #[test]
    fn serde_dialplan_type_inline() {
        let json = serde_json::to_string(&DialplanType::Inline).unwrap();
        assert_eq!(json, "\"inline\"");
        let parsed: DialplanType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DialplanType::Inline);
    }

    #[test]
    fn serde_variables_type() {
        let json = serde_json::to_string(&VariablesType::Enterprise).unwrap();
        assert_eq!(json, "\"enterprise\"");
        let parsed: VariablesType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, VariablesType::Enterprise);
    }

    #[test]
    fn serde_variables_flat_default() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("key1", "val1");
        vars.insert("key2", "val2");
        let json = serde_json::to_string(&vars).unwrap();
        // Default scope serializes as a flat map
        let parsed: Variables = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.vars_type, VariablesType::Default);
        assert_eq!(parsed.get("key1"), Some("val1"));
        assert_eq!(parsed.get("key2"), Some("val2"));
    }

    #[test]
    fn serde_variables_scoped_enterprise() {
        let mut vars = Variables::new(VariablesType::Enterprise);
        vars.insert("key1", "val1");
        let json = serde_json::to_string(&vars).unwrap();
        // Non-default scope serializes as {scope, vars}
        assert!(json.contains("\"enterprise\""));
        let parsed: Variables = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.vars_type, VariablesType::Enterprise);
        assert_eq!(parsed.get("key1"), Some("val1"));
    }

    #[test]
    fn serde_variables_flat_map_deserializes_as_default() {
        let json = r#"{"key1":"val1","key2":"val2"}"#;
        let vars: Variables = serde_json::from_str(json).unwrap();
        assert_eq!(vars.vars_type, VariablesType::Default);
        assert_eq!(vars.get("key1"), Some("val1"));
        assert_eq!(vars.get("key2"), Some("val2"));
    }

    #[test]
    fn serde_variables_scoped_deserializes() {
        let json = r#"{"scope":"channel","vars":{"k":"v"}}"#;
        let vars: Variables = serde_json::from_str(json).unwrap();
        assert_eq!(vars.vars_type, VariablesType::Channel);
        assert_eq!(vars.get("k"), Some("v"));
    }

    #[test]
    fn serde_application() {
        let app = Application::new("park", None::<&str>);
        let json = serde_json::to_string(&app).unwrap();
        let parsed: Application = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, app);
    }

    #[test]
    fn serde_application_with_args() {
        let app = Application::new("conference", Some("1"));
        let json = serde_json::to_string(&app).unwrap();
        let parsed: Application = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, app);
    }

    #[test]
    fn serde_application_skips_none_args() {
        let app = Application::new("park", None::<&str>);
        let json = serde_json::to_string(&app).unwrap();
        assert!(!json.contains("args"));
    }

    #[test]
    fn serde_application_list() {
        let list = ApplicationList(vec![
            Application::new("park", None::<&str>),
            Application::new("conference", Some("1")),
        ]);
        let json = serde_json::to_string(&list).unwrap();
        let parsed: ApplicationList = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, list);
    }

    #[test]
    fn serde_originate_round_trip() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate {
            endpoint: ep,
            applications: ApplicationList(vec![Application::new("park", None::<&str>)]),
            dialplan: Some(DialplanType::Xml),
            context: Some("default".into()),
            cid_name: Some("Test".into()),
            cid_num: Some("5551234".into()),
            timeout: Some(30),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let parsed: Originate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn serde_originate_skips_none_fields() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate {
            endpoint: ep,
            applications: ApplicationList(vec![Application::new("park", None::<&str>)]),
            dialplan: None,
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
        };
        let json = serde_json::to_string(&orig).unwrap();
        assert!(!json.contains("dialplan"));
        assert!(!json.contains("context"));
        assert!(!json.contains("cid_name"));
        assert!(!json.contains("cid_num"));
        assert!(!json.contains("timeout"));
    }

    #[test]
    fn serde_originate_to_wire_format() {
        let json = r#"{
            "endpoint": {"sofia": {"profile": "internal", "destination": "123@example.com"}},
            "applications": [{"name": "park"}],
            "dialplan": "xml",
            "context": "default"
        }"#;
        let orig: Originate = serde_json::from_str(json).unwrap();
        let wire = orig.to_string();
        assert!(wire.starts_with("originate"));
        assert!(wire.contains("sofia/internal/123@example.com"));
        assert!(wire.contains("&park()"));
        assert!(wire.contains("XML"));
    }
}
