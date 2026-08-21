#[derive(Clone, Copy)]
pub(crate) struct AudioToolFailure {
    pub(super) code: &'static str,
    pub(super) message: &'static str,
}

impl AudioToolFailure {
    const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }

    pub(crate) fn invalid_path() -> Self {
        Self::new(
            "audio_transcription_invalid_path",
            "Audio path must be a readable workspace-relative audio file.",
        )
    }

    pub(crate) fn invalid_source() -> Self {
        Self::new(
            "audio_transcription_invalid_source",
            "The audio source is empty or could not be read.",
        )
    }

    pub(crate) fn invalid_url() -> Self {
        Self::new(
            "audio_transcription_invalid_url",
            "Audio URLs must use HTTPS and resolve to a public address.",
        )
    }

    pub(crate) fn invalid_data() -> Self {
        Self::new(
            "audio_transcription_invalid_data",
            "The audio data is not valid Base64 or a supported Data URI.",
        )
    }

    pub(crate) fn too_large() -> Self {
        Self::new(
            "audio_transcription_too_large",
            "The audio source exceeds the selected transcription size limit.",
        )
    }

    pub(crate) fn duration_exceeded() -> Self {
        Self::new(
            "audio_transcription_duration_exceeded",
            "Flash transcription accepts audio up to two hours long.",
        )
    }

    pub(crate) fn unsupported_format() -> Self {
        Self::new(
            "audio_transcription_unsupported_format",
            "The audio file format is not supported.",
        )
    }

    pub(crate) fn invalid_arguments() -> Self {
        Self::new(
            "audio_transcription_invalid_arguments",
            "Audio transcription arguments are invalid.",
        )
    }

    pub(crate) fn converter_unavailable() -> Self {
        Self::new(
            "audio_transcription_converter_unavailable",
            "ffmpeg is required to convert this audio format but is not available.",
        )
    }

    pub(crate) fn conversion_failed() -> Self {
        Self::new(
            "audio_transcription_conversion_failed",
            "The audio source could not be converted to a supported format.",
        )
    }

    pub(crate) fn download_failed() -> Self {
        Self::new(
            "audio_transcription_download_failed",
            "The audio URL could not be downloaded.",
        )
    }

    pub(crate) fn invalid_response() -> Self {
        Self::new(
            "audio_transcription_invalid_response",
            "The transcription service returned an invalid response.",
        )
    }

    pub(crate) fn authentication_required() -> Self {
        Self::new(
            "audio_transcription_auth_required",
            "Sign in to iyw-claw before transcribing audio.",
        )
    }

    pub(crate) fn transport() -> Self {
        Self::new(
            "audio_transcription_transport_failed",
            "The transcription service could not be reached.",
        )
    }

    pub(crate) fn upload_failed() -> Self {
        Self::new(
            "audio_transcription_upload_failed",
            "The audio file could not be uploaded.",
        )
    }

    pub(crate) fn gateway(code: Option<&str>) -> Self {
        match code {
            Some("VOICE_INVALID_INPUT" | "UPLOAD_FILE_METADATA_MISMATCH") => Self::new(
                "audio_transcription_invalid_arguments",
                "The transcription service rejected the audio parameters.",
            ),
            Some("VOICE_PROVIDER_UNAVAILABLE" | "UPLOAD_STORAGE_UNAVAILABLE") => Self::new(
                "audio_transcription_provider_unavailable",
                "The transcription provider is not configured or temporarily unavailable.",
            ),
            Some("VOICE_PROVIDER_CAPABILITY_UNSUPPORTED") => Self::new(
                "audio_transcription_option_unsupported",
                "The selected transcription mode does not support these options.",
            ),
            Some("UPLOAD_FILE_NOT_FOUND" | "UPLOAD_FILE_NOT_OWNED") => Self::new(
                "audio_transcription_upload_invalid",
                "The managed audio upload is unavailable or not owned by this account.",
            ),
            Some("VOICE_TRANSCRIPTION_FAILED") => Self::new(
                "audio_transcription_provider_failed",
                "The upstream transcription provider rejected or could not process the audio.",
            ),
            _ => Self::new(
                "audio_transcription_request_failed",
                "The transcription service rejected the request.",
            ),
        }
    }
}
