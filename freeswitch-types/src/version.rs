//! FreeSWITCH version an application states it targets.

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
