// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Encode one message, or — for a reply too large for the channel — the
/// refusal that says so, so an oversized capture costs the caller one answer
/// rather than the connection.
pub(super) fn encode(message: &HelperMessage) -> Vec<u8> {
    let bytes = serde_json::to_vec(message).unwrap_or_default();
    if bytes.len() <= MAX_FRAME_BYTES {
        return bytes;
    }
    let id = match message {
        HelperMessage::Reply(reply) => reply.id,
        HelperMessage::Ready(_) => 0,
    };
    serde_json::to_vec(&HelperMessage::Reply(HelperReply::error(
        id,
        HelperError::failed(format!(
            "the answer was {} bytes, more than the {MAX_FRAME_BYTES} a reply can carry; ask for a \
             smaller image",
            bytes.len()
        )),
    )))
    .unwrap_or_default()
}

/// How long, once iyw-claw has gone, the helper waits for its driver to stop
/// before it exits anyway (the driver then sees its stdin close and exits on
/// its own). Long enough for a driver still starting to finish starting and
/// be stopped: past the file hash (after which a launch that meets a Stop
/// spawns nothing — see `DriverProc::launch`), a start is bounded by the
/// driver's handshake and configuration; one already running stops in
/// seconds.
pub(super) const SHUTDOWN_GRACE: Duration = Duration::from_secs(60);
