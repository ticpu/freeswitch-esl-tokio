//! FreeSWITCH version an application states it targets.

use std::cmp::Ordering;
use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

/// A FreeSWITCH version in its short form, `1.10.12` or `1.10.13-dev`.
///
/// Supplied by the application from its own configuration; nothing in this
/// crate reads it from the `FreeSWITCH-Version` event header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FreeswitchVersion {
    major: u32,
    minor: u32,
    micro: u32,
    dev: bool,
}

impl FreeswitchVersion {
    /// A release version.
    pub const fn new(major: u32, minor: u32, micro: u32) -> Self {
        Self {
            major,
            minor,
            micro,
            dev: false,
        }
    }

    /// The development build that precedes this release.
    pub const fn dev(mut self) -> Self {
        self.dev = true;
        self
    }

    /// Major component.
    pub fn major(&self) -> u32 {
        self.major
    }

    /// Minor component.
    pub fn minor(&self) -> u32 {
        self.minor
    }

    /// Micro component.
    pub fn micro(&self) -> u32 {
        self.micro
    }

    /// Whether this is a `-dev` build.
    pub fn is_dev(&self) -> bool {
        self.dev
    }
}

impl Ord for FreeswitchVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.micro)
            .cmp(&(other.major, other.minor, other.micro))
            .then_with(|| {
                other
                    .dev
                    .cmp(&self.dev)
            })
    }
}

impl PartialOrd for FreeswitchVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for FreeswitchVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.micro)?;
        if self.dev {
            f.write_str("-dev")?;
        }
        Ok(())
    }
}

impl FromStr for FreeswitchVersion {
    type Err = ParseFreeswitchVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (numbers, dev) = match s.split_once('-') {
            None => (s, false),
            Some((numbers, "dev")) => (numbers, true),
            Some((numbers, "release")) => (numbers, false),
            Some(_) => return Err(ParseFreeswitchVersionError::UnknownSuffix),
        };
        let mut parts = numbers.split('.');
        let major = component(&mut parts, "major")?;
        let minor = component(&mut parts, "minor")?;
        let micro = component(&mut parts, "micro")?;
        if parts
            .next()
            .is_some()
        {
            return Err(ParseFreeswitchVersionError::TrailingComponent);
        }
        Ok(Self {
            major,
            minor,
            micro,
            dev,
        })
    }
}

fn component(
    parts: &mut std::str::Split<'_, char>,
    name: &'static str,
) -> Result<u32, ParseFreeswitchVersionError> {
    parts
        .next()
        .ok_or(ParseFreeswitchVersionError::MissingComponent(name))?
        .parse()
        .map_err(|source| ParseFreeswitchVersionError::InvalidComponent {
            component: name,
            source,
        })
}

/// Why a string is not a FreeSWITCH short-form version. Names the part at fault,
/// never the input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseFreeswitchVersionError {
    /// A `major.minor.micro` component is absent; carries its name.
    MissingComponent(&'static str),
    /// A component is not an unsigned integer.
    InvalidComponent {
        /// Which component.
        component: &'static str,
        /// The integer parse failure.
        source: ParseIntError,
    },
    /// More than three dot-separated components.
    TrailingComponent,
    /// A suffix other than `-dev` or `-release`.
    UnknownSuffix,
}

impl fmt::Display for ParseFreeswitchVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingComponent(name) => {
                write!(f, "FreeSWITCH version lacks its {name} component")
            }
            Self::InvalidComponent { component, .. } => {
                write!(
                    f,
                    "FreeSWITCH version {component} component is not a number"
                )
            }
            Self::TrailingComponent => {
                f.write_str("FreeSWITCH version has components past major.minor.micro")
            }
            Self::UnknownSuffix => {
                f.write_str("FreeSWITCH version suffix is neither -dev nor -release")
            }
        }
    }
}

impl std::error::Error for ParseFreeswitchVersionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidComponent { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for FreeswitchVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FreeswitchVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_and_dev_short_forms() {
        let cases = [
            ("1.10.12", FreeswitchVersion::new(1, 10, 12)),
            ("1.10.12-release", FreeswitchVersion::new(1, 10, 12)),
            ("1.10.13-dev", FreeswitchVersion::new(1, 10, 13).dev()),
        ];
        for (input, want) in cases {
            assert_eq!(
                input
                    .parse::<FreeswitchVersion>()
                    .ok(),
                Some(want),
                "{input}"
            );
        }
        let dev: FreeswitchVersion = "1.10.13-dev"
            .parse()
            .unwrap();
        assert!(dev.is_dev());
        assert_eq!((dev.major(), dev.minor(), dev.micro()), (1, 10, 13));
    }

    /// A development build precedes the release it becomes, so a `-dev` version
    /// must never sort above that release.
    #[test]
    fn a_dev_build_sorts_below_its_release() {
        assert!(FreeswitchVersion::new(1, 10, 13).dev() < FreeswitchVersion::new(1, 10, 13));
        assert!(FreeswitchVersion::new(1, 10, 12) < FreeswitchVersion::new(1, 10, 13).dev());
        assert!(FreeswitchVersion::new(1, 9, 99) < FreeswitchVersion::new(1, 10, 0));
    }

    #[test]
    fn displays_the_short_form() {
        for input in ["1.10.12", "1.10.13-dev"] {
            let version: FreeswitchVersion = input
                .parse()
                .unwrap();
            assert_eq!(version.to_string(), input);
        }
    }

    #[test]
    fn malformed_versions_are_refused_without_quoting_them() {
        for (input, fragment) in [
            ("1.10", "1.10"),
            ("1.10.x7", "x7"),
            ("1.10.12-beta9", "beta9"),
            ("1.10.12.4", "12.4"),
            ("v1.10.12", "v1"),
        ] {
            let err = input
                .parse::<FreeswitchVersion>()
                .expect_err(input)
                .to_string();
            assert!(!err.contains(fragment), "error quoted {input}: {err}");
        }
        assert!(""
            .parse::<FreeswitchVersion>()
            .is_err());
    }

    #[test]
    fn serde_uses_the_short_form() {
        let version = FreeswitchVersion::new(1, 10, 13).dev();
        let json = serde_json::to_string(&version).unwrap();
        assert_eq!(json, r#""1.10.13-dev""#);
        let back: FreeswitchVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(back, version);
    }
}
