use std::fmt;

use super::{
    after_prefix, check_field, parse_leg, undeliverable, write_module_text, write_variables,
    EndpointFieldFault,
};
use crate::commands::originate::OriginateError;
use crate::commands::variables::{DialStringCarrier, DialStringTarget, Variables};

/// Audio device endpoint for portaudio, pulseaudio, or ALSA modules.
///
/// Wire format: `{module}[/{destination}]` where destination is typically
/// empty or `auto_answer` (recognized by portaudio and pulseaudio).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "config::AudioEndpoint"))]
#[non_exhaustive]
pub struct AudioEndpoint {
    /// Destination string (e.g. `auto_answer`). `None` for bare module name.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub destination: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub variables: Option<Variables>,
}

impl AudioEndpoint {
    /// Create a new audio endpoint with no destination.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the destination string.
    pub fn with_destination(mut self, destination: impl Into<String>) -> Self {
        self.destination = Some(destination.into());
        self
    }

    /// Format with the given module prefix (`portaudio`, `pulseaudio`, `alsa`),
    /// which the [`Endpoint`](super::Endpoint) variant supplies because this
    /// struct does not record which of the three modules it belongs to.
    ///
    /// Renders for [`DialStringCarrier::EslApi`]; the [`Endpoint`](super::Endpoint)
    /// variants render for whichever carrier they were asked for.
    pub fn fmt_with_prefix(&self, f: &mut fmt::Formatter<'_>, prefix: &str) -> fmt::Result {
        self.write_with_prefix(f, prefix, DialStringCarrier::EslApi.into())
    }

    pub(super) fn write_with_prefix(
        &self,
        f: &mut fmt::Formatter<'_>,
        prefix: &str,
        target: DialStringTarget,
    ) -> fmt::Result {
        write_variables(f, &self.variables, target)?;
        write_module_text(f, &self.module_text(prefix), target)
    }

    /// The text after the variable block, as the `prefix` module receives it.
    pub(crate) fn module_text(&self, prefix: &str) -> String {
        match &self.destination {
            Some(dest) => format!("{prefix}/{dest}"),
            None => prefix.to_owned(),
        }
    }

    /// Refuse `:_:` and an empty destination, which the module reads as none.
    pub(crate) fn check_deliverable(&self) -> Result<(), OriginateError> {
        match self
            .destination
            .as_deref()
        {
            Some("") => Err(undeliverable(
                "audio",
                "destination",
                EndpointFieldFault::EmptyReadsAsDefault,
            )),
            Some(destination) => check_field("audio", "destination", destination, &[]),
            None => Ok(()),
        }
    }

    /// Parse from a dial string with the given module prefix.
    pub fn parse_with_prefix(s: &str, prefix: &str) -> Result<Self, OriginateError> {
        let (variables, ep) = parse_leg(s, DialStringCarrier::EslApi.into(), |text| {
            Self::parse_bare(text, prefix)
        })?;
        Ok(Self { variables, ..ep })
    }

    pub(crate) fn parse_bare(text: &str, prefix: &str) -> Result<Self, OriginateError> {
        let rest = after_prefix(text, prefix, prefix)?;
        let ep = Self {
            destination: rest
                .strip_prefix('/')
                .filter(|d| !d.is_empty())
                .map(str::to_string),
            variables: None,
        };
        ep.check_deliverable()?;
        Ok(ep)
    }
}

impl_dial_string_with_variables!(AudioEndpoint);

#[cfg(feature = "serde")]
mod config {
    use crate::commands::variables::Variables;

    #[derive(serde::Deserialize)]
    pub(super) struct AudioEndpoint {
        #[serde(default)]
        pub(super) destination: Option<String>,
        #[serde(default)]
        pub(super) variables: Option<Variables>,
    }
}

#[cfg(feature = "serde")]
impl TryFrom<config::AudioEndpoint> for AudioEndpoint {
    type Error = OriginateError;

    fn try_from(config: config::AudioEndpoint) -> Result<Self, Self::Error> {
        let ep = Self {
            destination: config.destination,
            variables: config.variables,
        };
        ep.check_deliverable()?;
        Ok(ep)
    }
}

/// **Warning:** This `Display` impl exists only to satisfy the `DialString: Display`
/// trait bound. The `"audio"` prefix is not a valid FreeSWITCH endpoint.
/// Always use `AudioEndpoint` through [`Endpoint::PortAudio`](super::Endpoint::PortAudio),
/// [`Endpoint::PulseAudio`](super::Endpoint::PulseAudio), or
/// [`Endpoint::Alsa`](super::Endpoint::Alsa) which call
/// [`fmt_with_prefix`](AudioEndpoint::fmt_with_prefix) with the correct module name.
impl fmt::Display for AudioEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_prefix(f, "audio")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_prefix_portaudio() {
        let ep = AudioEndpoint::parse_with_prefix("portaudio/auto_answer", "portaudio").unwrap();
        assert_eq!(
            ep.destination
                .as_deref(),
            Some("auto_answer")
        );
        assert!(ep
            .variables
            .is_none());
    }

    #[test]
    fn parse_with_prefix_bare() {
        let ep = AudioEndpoint::parse_with_prefix("alsa", "alsa").unwrap();
        assert!(ep
            .destination
            .is_none());
    }

    #[test]
    fn parse_with_prefix_trailing_slash() {
        let ep = AudioEndpoint::parse_with_prefix("pulseaudio/", "pulseaudio").unwrap();
        assert!(ep
            .destination
            .is_none());
    }

    #[test]
    fn parse_with_prefix_wrong_module() {
        let result = AudioEndpoint::parse_with_prefix("portaudio/x", "alsa");
        assert!(result.is_err());
    }

    /// A module name is a whole path segment. Accepting a longer one and
    /// keeping the prefix drops the rest of the dial string without a word.
    #[test]
    fn parse_with_prefix_rejects_a_longer_module_name() {
        assert!(AudioEndpoint::parse_with_prefix("alsafoo/bar", "alsa").is_err());
        assert!("alsafoo/bar"
            .parse::<super::super::Endpoint>()
            .is_err());
    }
}
