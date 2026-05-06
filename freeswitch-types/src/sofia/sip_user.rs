//! SIP user ping status enum.

wire_enum! {
    /// SIP user ping status from `sofia::sip_user_state` events.
    ///
    /// The `Ping-Status` header value, mapping to `sofia_sip_user_status_name()`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum SipUserPingStatus {
        Unreachable => "UNREACHABLE",
        Reachable => "REACHABLE",
        Invalid => "INVALID",
    }
    error ParseSipUserPingStatusError("sip user ping status");
    tests: sip_user_ping_status_wire_tests;
}
