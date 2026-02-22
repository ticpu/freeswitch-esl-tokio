//! Typed channel variable names from FreeSWITCH core.

/// Error returned when parsing an unrecognized channel variable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseChannelVariableError(pub String);

impl std::fmt::Display for ParseChannelVariableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown channel variable: {}", self.0)
    }
}

impl std::error::Error for ParseChannelVariableError {}

define_header_enum! {
    error_type: ParseChannelVariableError,
    /// Core FreeSWITCH channel variable names (the part after the `variable_` prefix).
    ///
    /// Use with [`EslEvent::variable()`] for type-safe lookups. Only includes
    /// variables set by the FreeSWITCH core — module-specific variables (SIP,
    /// conference, etc.) belong in separate enums.
    pub enum ChannelVariable {
        Uuid => "uuid",
        Direction => "direction",
        CallerIdName => "caller_id_name",
        CallerIdNumber => "caller_id_number",
        CalleeIdName => "callee_id_name",
        CalleeIdNumber => "callee_id_number",
        EffectiveCallerIdName => "effective_caller_id_name",
        EffectiveCallerIdNumber => "effective_caller_id_number",
        EffectiveCalleeIdName => "effective_callee_id_name",
        EffectiveCalleeIdNumber => "effective_callee_id_number",
        Ani => "ani",
        Rdnis => "rdnis",
        DestinationNumber => "destination_number",
        ChannelName => "channel_name",
        Context => "context",
        Dialplan => "dialplan",
        HangupCause => "hangup_cause",
        HangupCauseQ850 => "hangup_cause_q850",
        BridgeUuid => "bridge_uuid",
        SignalBond => "signal_bond",
        LastBridgeTo => "last_bridge_to",
        ReadCodec => "read_codec",
        ReadRate => "read_rate",
        WriteCodec => "write_codec",
        WriteRate => "write_rate",
        CodecString => "codec_string",
        AbsoluteCodecString => "absolute_codec_string",
        Accountcode => "accountcode",
        TollAllow => "toll_allow",
        DomainName => "domain_name",
        UserName => "user_name",
        UserContext => "user_context",
        PresenceId => "presence_id",
        EndpointDisposition => "endpoint_disposition",
        CurrentApplication => "current_application",
        CurrentApplicationData => "current_application_data",
        LastApp => "last_app",
        LastArg => "last_arg",
        TransferSource => "transfer_source",
        TransferHistory => "transfer_history",
        HoldMusic => "hold_music",
        ExportVars => "export_vars",
        BridgeExportVars => "bridge_export_vars",
        CallTimeout => "call_timeout",
        OriginateTimeout => "originate_timeout",
        ContinueOnFail => "continue_on_fail",
        HangupAfterBridge => "hangup_after_bridge",
        ParkAfterBridge => "park_after_bridge",
        IgnoreEarlyMedia => "ignore_early_media",
        ProgressTimeout => "progress_timeout",
        OriginationCallerIdName => "origination_caller_id_name",
        OriginationCallerIdNumber => "origination_caller_id_number",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trip() {
        assert_eq!(ChannelVariable::Direction.to_string(), "direction");
        assert_eq!(ChannelVariable::CallerIdName.to_string(), "caller_id_name");
        assert_eq!(
            ChannelVariable::EffectiveCallerIdNumber.to_string(),
            "effective_caller_id_number"
        );
        assert_eq!(
            ChannelVariable::HangupCauseQ850.to_string(),
            "hangup_cause_q850"
        );
    }

    #[test]
    fn as_ref_str() {
        let v: &str = ChannelVariable::BridgeUuid.as_ref();
        assert_eq!(v, "bridge_uuid");
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!(
            "direction".parse::<ChannelVariable>(),
            Ok(ChannelVariable::Direction)
        );
        assert_eq!(
            "DIRECTION".parse::<ChannelVariable>(),
            Ok(ChannelVariable::Direction)
        );
        assert_eq!(
            "Direction".parse::<ChannelVariable>(),
            Ok(ChannelVariable::Direction)
        );
    }

    #[test]
    fn from_str_unknown() {
        let err = "nonexistent_var".parse::<ChannelVariable>();
        assert!(err.is_err());
        assert_eq!(
            err.unwrap_err()
                .to_string(),
            "unknown channel variable: nonexistent_var"
        );
    }

    #[test]
    fn from_str_round_trip_all_variants() {
        let variants = [
            ChannelVariable::Uuid,
            ChannelVariable::Direction,
            ChannelVariable::CallerIdName,
            ChannelVariable::CallerIdNumber,
            ChannelVariable::CalleeIdName,
            ChannelVariable::CalleeIdNumber,
            ChannelVariable::EffectiveCallerIdName,
            ChannelVariable::EffectiveCallerIdNumber,
            ChannelVariable::EffectiveCalleeIdName,
            ChannelVariable::EffectiveCalleeIdNumber,
            ChannelVariable::Ani,
            ChannelVariable::Rdnis,
            ChannelVariable::DestinationNumber,
            ChannelVariable::ChannelName,
            ChannelVariable::Context,
            ChannelVariable::Dialplan,
            ChannelVariable::HangupCause,
            ChannelVariable::HangupCauseQ850,
            ChannelVariable::BridgeUuid,
            ChannelVariable::SignalBond,
            ChannelVariable::LastBridgeTo,
            ChannelVariable::ReadCodec,
            ChannelVariable::ReadRate,
            ChannelVariable::WriteCodec,
            ChannelVariable::WriteRate,
            ChannelVariable::CodecString,
            ChannelVariable::AbsoluteCodecString,
            ChannelVariable::Accountcode,
            ChannelVariable::TollAllow,
            ChannelVariable::DomainName,
            ChannelVariable::UserName,
            ChannelVariable::UserContext,
            ChannelVariable::PresenceId,
            ChannelVariable::EndpointDisposition,
            ChannelVariable::CurrentApplication,
            ChannelVariable::CurrentApplicationData,
            ChannelVariable::LastApp,
            ChannelVariable::LastArg,
            ChannelVariable::TransferSource,
            ChannelVariable::TransferHistory,
            ChannelVariable::HoldMusic,
            ChannelVariable::ExportVars,
            ChannelVariable::BridgeExportVars,
            ChannelVariable::CallTimeout,
            ChannelVariable::OriginateTimeout,
            ChannelVariable::ContinueOnFail,
            ChannelVariable::HangupAfterBridge,
            ChannelVariable::ParkAfterBridge,
            ChannelVariable::IgnoreEarlyMedia,
            ChannelVariable::ProgressTimeout,
            ChannelVariable::OriginationCallerIdName,
            ChannelVariable::OriginationCallerIdNumber,
        ];
        for v in variants {
            let wire = v.to_string();
            let parsed: ChannelVariable = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, v, "round-trip failed for {wire}");
        }
    }
}
