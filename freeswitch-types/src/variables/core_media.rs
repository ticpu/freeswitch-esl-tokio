//! RTP media statistics variables set by `switch_core_media_set_stats()`.

/// Error returned when parsing an unrecognized core media variable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCoreMediaVariableError(pub String);

impl std::fmt::Display for ParseCoreMediaVariableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown core media variable: {}", self.0)
    }
}

impl std::error::Error for ParseCoreMediaVariableError {}

sip_header::define_header_enum! {
    error_type: ParseCoreMediaVariableError,
    /// RTP media statistics channel variable names (the part after the `variable_` prefix).
    ///
    /// Set by `switch_core_media_set_stats()` at channel teardown for each media type
    /// (audio, video, text). Use with
    /// [`HeaderLookup::variable()`](crate::HeaderLookup::variable) for type-safe lookups.
    pub enum CoreMediaVariable {
        // --- Audio Inbound ---
        RtpAudioInRawBytes => "rtp_audio_in_raw_bytes",
        RtpAudioInMediaBytes => "rtp_audio_in_media_bytes",
        RtpAudioInPacketCount => "rtp_audio_in_packet_count",
        RtpAudioInMediaPacketCount => "rtp_audio_in_media_packet_count",
        RtpAudioInSkipPacketCount => "rtp_audio_in_skip_packet_count",
        RtpAudioInJitterPacketCount => "rtp_audio_in_jitter_packet_count",
        RtpAudioInDtmfPacketCount => "rtp_audio_in_dtmf_packet_count",
        RtpAudioInCngPacketCount => "rtp_audio_in_cng_packet_count",
        RtpAudioInFlushPacketCount => "rtp_audio_in_flush_packet_count",
        RtpAudioInLargestJbSize => "rtp_audio_in_largest_jb_size",
        RtpAudioInJitterMinVariance => "rtp_audio_in_jitter_min_variance",
        RtpAudioInJitterMaxVariance => "rtp_audio_in_jitter_max_variance",
        RtpAudioInJitterLossRate => "rtp_audio_in_jitter_loss_rate",
        RtpAudioInJitterBurstRate => "rtp_audio_in_jitter_burst_rate",
        RtpAudioInMeanInterval => "rtp_audio_in_mean_interval",
        RtpAudioInFlawTotal => "rtp_audio_in_flaw_total",
        RtpAudioInQualityPercentage => "rtp_audio_in_quality_percentage",
        RtpAudioInMos => "rtp_audio_in_mos",

        // --- Audio Outbound ---
        RtpAudioOutRawBytes => "rtp_audio_out_raw_bytes",
        RtpAudioOutMediaBytes => "rtp_audio_out_media_bytes",
        RtpAudioOutPacketCount => "rtp_audio_out_packet_count",
        RtpAudioOutMediaPacketCount => "rtp_audio_out_media_packet_count",
        RtpAudioOutSkipPacketCount => "rtp_audio_out_skip_packet_count",
        RtpAudioOutDtmfPacketCount => "rtp_audio_out_dtmf_packet_count",
        RtpAudioOutCngPacketCount => "rtp_audio_out_cng_packet_count",

        // --- Audio RTCP ---
        RtpAudioRtcpPacketCount => "rtp_audio_rtcp_packet_count",
        RtpAudioRtcpOctetCount => "rtp_audio_rtcp_octet_count",

        // --- Video Inbound ---
        RtpVideoInRawBytes => "rtp_video_in_raw_bytes",
        RtpVideoInMediaBytes => "rtp_video_in_media_bytes",
        RtpVideoInPacketCount => "rtp_video_in_packet_count",
        RtpVideoInMediaPacketCount => "rtp_video_in_media_packet_count",
        RtpVideoInSkipPacketCount => "rtp_video_in_skip_packet_count",
        RtpVideoInJitterPacketCount => "rtp_video_in_jitter_packet_count",
        RtpVideoInDtmfPacketCount => "rtp_video_in_dtmf_packet_count",
        RtpVideoInCngPacketCount => "rtp_video_in_cng_packet_count",
        RtpVideoInFlushPacketCount => "rtp_video_in_flush_packet_count",
        RtpVideoInLargestJbSize => "rtp_video_in_largest_jb_size",
        RtpVideoInJitterMinVariance => "rtp_video_in_jitter_min_variance",
        RtpVideoInJitterMaxVariance => "rtp_video_in_jitter_max_variance",
        RtpVideoInJitterLossRate => "rtp_video_in_jitter_loss_rate",
        RtpVideoInJitterBurstRate => "rtp_video_in_jitter_burst_rate",
        RtpVideoInMeanInterval => "rtp_video_in_mean_interval",
        RtpVideoInFlawTotal => "rtp_video_in_flaw_total",
        RtpVideoInQualityPercentage => "rtp_video_in_quality_percentage",
        RtpVideoInMos => "rtp_video_in_mos",

        // --- Video Outbound ---
        RtpVideoOutRawBytes => "rtp_video_out_raw_bytes",
        RtpVideoOutMediaBytes => "rtp_video_out_media_bytes",
        RtpVideoOutPacketCount => "rtp_video_out_packet_count",
        RtpVideoOutMediaPacketCount => "rtp_video_out_media_packet_count",
        RtpVideoOutSkipPacketCount => "rtp_video_out_skip_packet_count",
        RtpVideoOutDtmfPacketCount => "rtp_video_out_dtmf_packet_count",
        RtpVideoOutCngPacketCount => "rtp_video_out_cng_packet_count",

        // --- Video RTCP ---
        RtpVideoRtcpPacketCount => "rtp_video_rtcp_packet_count",
        RtpVideoRtcpOctetCount => "rtp_video_rtcp_octet_count",

        // --- Text Inbound ---
        RtpTextInRawBytes => "rtp_text_in_raw_bytes",
        RtpTextInMediaBytes => "rtp_text_in_media_bytes",
        RtpTextInPacketCount => "rtp_text_in_packet_count",
        RtpTextInMediaPacketCount => "rtp_text_in_media_packet_count",
        RtpTextInSkipPacketCount => "rtp_text_in_skip_packet_count",
        RtpTextInJitterPacketCount => "rtp_text_in_jitter_packet_count",
        RtpTextInDtmfPacketCount => "rtp_text_in_dtmf_packet_count",
        RtpTextInCngPacketCount => "rtp_text_in_cng_packet_count",
        RtpTextInFlushPacketCount => "rtp_text_in_flush_packet_count",
        RtpTextInLargestJbSize => "rtp_text_in_largest_jb_size",
        RtpTextInJitterMinVariance => "rtp_text_in_jitter_min_variance",
        RtpTextInJitterMaxVariance => "rtp_text_in_jitter_max_variance",
        RtpTextInJitterLossRate => "rtp_text_in_jitter_loss_rate",
        RtpTextInJitterBurstRate => "rtp_text_in_jitter_burst_rate",
        RtpTextInMeanInterval => "rtp_text_in_mean_interval",
        RtpTextInFlawTotal => "rtp_text_in_flaw_total",
        RtpTextInQualityPercentage => "rtp_text_in_quality_percentage",
        RtpTextInMos => "rtp_text_in_mos",

        // --- Text Outbound ---
        RtpTextOutRawBytes => "rtp_text_out_raw_bytes",
        RtpTextOutMediaBytes => "rtp_text_out_media_bytes",
        RtpTextOutPacketCount => "rtp_text_out_packet_count",
        RtpTextOutMediaPacketCount => "rtp_text_out_media_packet_count",
        RtpTextOutSkipPacketCount => "rtp_text_out_skip_packet_count",
        RtpTextOutDtmfPacketCount => "rtp_text_out_dtmf_packet_count",
        RtpTextOutCngPacketCount => "rtp_text_out_cng_packet_count",

        // --- Text RTCP ---
        RtpTextRtcpPacketCount => "rtp_text_rtcp_packet_count",
        RtpTextRtcpOctetCount => "rtp_text_rtcp_octet_count",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trip() {
        assert_eq!(
            CoreMediaVariable::RtpAudioInRawBytes.to_string(),
            "rtp_audio_in_raw_bytes"
        );
        assert_eq!(
            CoreMediaVariable::RtpVideoOutPacketCount.to_string(),
            "rtp_video_out_packet_count"
        );
        assert_eq!(
            CoreMediaVariable::RtpTextRtcpOctetCount.to_string(),
            "rtp_text_rtcp_octet_count"
        );
    }

    #[test]
    fn from_str_round_trip_sample() {
        let variants = [
            CoreMediaVariable::RtpAudioInMos,
            CoreMediaVariable::RtpAudioOutCngPacketCount,
            CoreMediaVariable::RtpAudioRtcpPacketCount,
            CoreMediaVariable::RtpVideoInQualityPercentage,
            CoreMediaVariable::RtpVideoOutRawBytes,
            CoreMediaVariable::RtpVideoRtcpOctetCount,
            CoreMediaVariable::RtpTextInFlawTotal,
            CoreMediaVariable::RtpTextOutMediaBytes,
            CoreMediaVariable::RtpTextRtcpPacketCount,
        ];
        for v in variants {
            let wire = v.to_string();
            let parsed: CoreMediaVariable = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, v, "round-trip failed for {wire}");
        }
    }
}
