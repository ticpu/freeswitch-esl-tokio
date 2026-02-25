//! Bridge dial string builder for multi-endpoint bridge commands.
//!
//! Supports simultaneous ring (`,`) and sequential failover (`|`)
//! with per-endpoint channel variables and global default variables.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::endpoint::Endpoint;
use super::originate::{OriginateError, Variables};

/// Typed bridge dial string.
///
/// Format: `{global_vars}[ep1_vars]ep1,[ep2_vars]ep2|[ep3_vars]ep3`
///
/// - `,` separates endpoints rung simultaneously (within a group)
/// - `|` separates groups tried sequentially (failover)
/// - Each endpoint may have channel-scope `[variables]`
/// - Global `{variables}` apply to all endpoints
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeDialString {
    /// Default-scope variables applied to all endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Variables>,
    /// Sequential failover groups (`|`-separated). Within each group,
    /// endpoints ring simultaneously (`,`-separated).
    pub groups: Vec<Vec<Endpoint>>,
}

impl fmt::Display for BridgeDialString {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("BridgeDialString::Display")
    }
}

impl FromStr for BridgeDialString {
    type Err = OriginateError;

    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        todo!("BridgeDialString::FromStr")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::endpoint::{ErrorEndpoint, LoopbackEndpoint, SofiaEndpoint, SofiaGateway};
    use crate::commands::originate::VariablesType;

    // === Display ===

    #[test]
    fn display_single_endpoint() {
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![vec![Endpoint::SofiaGateway(SofiaGateway {
                gateway: "my_provider".into(),
                destination: "18005551234".into(),
                profile: None,
                variables: None,
            })]],
        };
        assert_eq!(bridge.to_string(), "sofia/gateway/my_provider/18005551234");
    }

    #[test]
    fn display_simultaneous_ring() {
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![vec![
                Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "primary".into(),
                    destination: "18005551234".into(),
                    profile: None,
                    variables: None,
                }),
                Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "secondary".into(),
                    destination: "18005551234".into(),
                    profile: None,
                    variables: None,
                }),
            ]],
        };
        assert_eq!(
            bridge.to_string(),
            "sofia/gateway/primary/18005551234,sofia/gateway/secondary/18005551234"
        );
    }

    #[test]
    fn display_sequential_failover() {
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![
                vec![Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "primary".into(),
                    destination: "18005551234".into(),
                    profile: None,
                    variables: None,
                })],
                vec![Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "backup".into(),
                    destination: "18005551234".into(),
                    profile: None,
                    variables: None,
                })],
            ],
        };
        assert_eq!(
            bridge.to_string(),
            "sofia/gateway/primary/18005551234|sofia/gateway/backup/18005551234"
        );
    }

    #[test]
    fn display_mixed_simultaneous_and_sequential() {
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![
                vec![
                    Endpoint::SofiaGateway(SofiaGateway {
                        gateway: "primary".into(),
                        destination: "1234".into(),
                        profile: None,
                        variables: None,
                    }),
                    Endpoint::SofiaGateway(SofiaGateway {
                        gateway: "secondary".into(),
                        destination: "1234".into(),
                        profile: None,
                        variables: None,
                    }),
                ],
                vec![Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "backup".into(),
                    destination: "1234".into(),
                    profile: None,
                    variables: None,
                })],
            ],
        };
        assert_eq!(
            bridge.to_string(),
            "sofia/gateway/primary/1234,sofia/gateway/secondary/1234|sofia/gateway/backup/1234"
        );
    }

    #[test]
    fn display_with_global_variables() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("hangup_after_bridge", "true");
        let bridge = BridgeDialString {
            variables: Some(vars),
            groups: vec![vec![Endpoint::Sofia(SofiaEndpoint {
                profile: "internal".into(),
                destination: "1000@domain".into(),
                variables: None,
            })]],
        };
        assert_eq!(
            bridge.to_string(),
            "{hangup_after_bridge=true}sofia/internal/1000@domain"
        );
    }

    #[test]
    fn display_with_per_endpoint_variables() {
        let mut ep_vars = Variables::new(VariablesType::Channel);
        ep_vars.insert("leg_timeout", "30");
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![vec![
                Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "gw1".into(),
                    destination: "1234".into(),
                    profile: None,
                    variables: Some(ep_vars),
                }),
                Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "gw2".into(),
                    destination: "1234".into(),
                    profile: None,
                    variables: None,
                }),
            ]],
        };
        assert_eq!(
            bridge.to_string(),
            "[leg_timeout=30]sofia/gateway/gw1/1234,sofia/gateway/gw2/1234"
        );
    }

    #[test]
    fn display_with_error_endpoint_failover() {
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![
                vec![Endpoint::SofiaGateway(SofiaGateway {
                    gateway: "primary".into(),
                    destination: "1234".into(),
                    profile: None,
                    variables: None,
                })],
                vec![Endpoint::Error(ErrorEndpoint {
                    cause: "user_busy".into(),
                })],
            ],
        };
        assert_eq!(
            bridge.to_string(),
            "sofia/gateway/primary/1234|error/user_busy"
        );
    }

    #[test]
    fn display_with_loopback() {
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![vec![Endpoint::Loopback(LoopbackEndpoint {
                extension: "9199".into(),
                context: "default".into(),
                variables: None,
            })]],
        };
        assert_eq!(bridge.to_string(), "loopback/9199/default");
    }

    // === FromStr ===

    #[test]
    fn from_str_single_endpoint() {
        let bridge: BridgeDialString = "sofia/gateway/my_provider/18005551234"
            .parse()
            .unwrap();
        assert_eq!(
            bridge
                .groups
                .len(),
            1
        );
        assert_eq!(bridge.groups[0].len(), 1);
        assert!(bridge
            .variables
            .is_none());
    }

    #[test]
    fn from_str_simultaneous_ring() {
        let bridge: BridgeDialString = "sofia/gateway/primary/1234,sofia/gateway/secondary/1234"
            .parse()
            .unwrap();
        assert_eq!(
            bridge
                .groups
                .len(),
            1
        );
        assert_eq!(bridge.groups[0].len(), 2);
    }

    #[test]
    fn from_str_sequential_failover() {
        let bridge: BridgeDialString = "sofia/gateway/primary/1234|sofia/gateway/backup/1234"
            .parse()
            .unwrap();
        assert_eq!(
            bridge
                .groups
                .len(),
            2
        );
        assert_eq!(bridge.groups[0].len(), 1);
        assert_eq!(bridge.groups[1].len(), 1);
    }

    #[test]
    fn from_str_mixed() {
        let bridge: BridgeDialString =
            "sofia/gateway/primary/1234,sofia/gateway/secondary/1234|sofia/gateway/backup/1234"
                .parse()
                .unwrap();
        assert_eq!(
            bridge
                .groups
                .len(),
            2
        );
        assert_eq!(bridge.groups[0].len(), 2);
        assert_eq!(bridge.groups[1].len(), 1);
    }

    #[test]
    fn from_str_with_global_variables() {
        let bridge: BridgeDialString = "{hangup_after_bridge=true}sofia/internal/1000@domain"
            .parse()
            .unwrap();
        assert!(bridge
            .variables
            .is_some());
        assert_eq!(
            bridge
                .variables
                .as_ref()
                .unwrap()
                .get("hangup_after_bridge"),
            Some("true")
        );
        assert_eq!(
            bridge
                .groups
                .len(),
            1
        );
        assert_eq!(bridge.groups[0].len(), 1);
    }

    #[test]
    fn from_str_with_per_endpoint_variables() {
        let bridge: BridgeDialString =
            "[leg_timeout=30]sofia/gateway/gw1/1234,sofia/gateway/gw2/1234"
                .parse()
                .unwrap();
        assert_eq!(
            bridge
                .groups
                .len(),
            1
        );
        assert_eq!(bridge.groups[0].len(), 2);
        let ep = &bridge.groups[0][0];
        if let Endpoint::SofiaGateway(gw) = ep {
            assert!(gw
                .variables
                .is_some());
        } else {
            panic!("expected SofiaGateway");
        }
    }

    #[test]
    fn from_str_round_trip_single() {
        let input = "sofia/gateway/my_provider/18005551234";
        let bridge: BridgeDialString = input
            .parse()
            .unwrap();
        assert_eq!(bridge.to_string(), input);
    }

    #[test]
    fn from_str_round_trip_mixed() {
        let input =
            "sofia/gateway/primary/1234,sofia/gateway/secondary/1234|sofia/gateway/backup/1234";
        let bridge: BridgeDialString = input
            .parse()
            .unwrap();
        assert_eq!(bridge.to_string(), input);
    }

    #[test]
    fn from_str_round_trip_with_global_vars() {
        let input = "{hangup_after_bridge=true}sofia/internal/1000@domain";
        let bridge: BridgeDialString = input
            .parse()
            .unwrap();
        assert_eq!(bridge.to_string(), input);
    }

    // === Serde ===

    #[test]
    fn serde_round_trip_single() {
        let bridge = BridgeDialString {
            variables: None,
            groups: vec![vec![Endpoint::SofiaGateway(SofiaGateway {
                gateway: "my_provider".into(),
                destination: "18005551234".into(),
                profile: None,
                variables: None,
            })]],
        };
        let json = serde_json::to_string(&bridge).unwrap();
        let parsed: BridgeDialString = serde_json::from_str(&json).unwrap();
        assert_eq!(bridge, parsed);
    }

    #[test]
    fn serde_round_trip_multi_group() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("hangup_after_bridge", "true");
        let bridge = BridgeDialString {
            variables: Some(vars),
            groups: vec![
                vec![
                    Endpoint::SofiaGateway(SofiaGateway {
                        gateway: "primary".into(),
                        destination: "1234".into(),
                        profile: None,
                        variables: None,
                    }),
                    Endpoint::SofiaGateway(SofiaGateway {
                        gateway: "secondary".into(),
                        destination: "1234".into(),
                        profile: None,
                        variables: None,
                    }),
                ],
                vec![Endpoint::Error(ErrorEndpoint {
                    cause: "user_busy".into(),
                })],
            ],
        };
        let json = serde_json::to_string(&bridge).unwrap();
        let parsed: BridgeDialString = serde_json::from_str(&json).unwrap();
        assert_eq!(bridge, parsed);
    }

    #[test]
    fn serde_to_display_wire_format() {
        let json = r#"{
            "groups": [[{
                "sofia_gateway": {
                    "gateway": "my_gw",
                    "destination": "18005551234"
                }
            }]]
        }"#;
        let bridge: BridgeDialString = serde_json::from_str(json).unwrap();
        assert_eq!(bridge.to_string(), "sofia/gateway/my_gw/18005551234");
    }
}
