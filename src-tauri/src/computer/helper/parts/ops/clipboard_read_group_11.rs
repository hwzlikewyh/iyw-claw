// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The clipboard's text, while the clipboard is still as `expect` names it —
/// what an agent put there — and holds nothing concealed.
pub async fn clipboard_read(driver: &DriverProc, expect: u64) -> Result<RawClipboard, HelperError> {
    let not_yours = || {
        HelperError::new(
            HelperErrorCode::PasteRefused,
            "The clipboard holds what the user put there, not what you copied from a window you \
             may read or wrote yourself, so it is not read for you.",
        )
    };
    let now = crate::computer::helper::clipboard::stamp(driver).await?;
    if now.value != expect || now.concealed {
        return Err(not_yours());
    }
    let result = call(
        driver,
        "clipboard_read",
        json!({ "include_text": true }),
        LIST_TIMEOUT,
    )
    .await?;
    // What was read is what was checked only if nothing came between.
    let after = crate::computer::helper::clipboard::stamp(driver).await?;
    if after.value != expect || after.concealed {
        return Err(not_yours());
    }
    let text = result
        .structured
        .as_ref()
        .and_then(|s| string(s, "text"))
        .map(|text| text.chars().take(MAX_CLIPBOARD_CHARS).collect());
    Ok(RawClipboard { text })
}

/// Put `text` on the clipboard; answers with the clipboard's stamp after —
/// what the agent itself put there, until anything else does. The stamp is
/// the agent's only once the clipboard is read back holding that text, with
/// the stamp the same before and after the reading: something copied in the
/// moment between would otherwise be taken for the agent's.
pub async fn clipboard_write(driver: &DriverProc, text: &str) -> Result<u64, HelperError> {
    call(
        driver,
        "clipboard_write",
        json!({ "text": text }),
        LIST_TIMEOUT,
    )
    .await?;
    let before = crate::computer::helper::clipboard::stamp(driver).await?;
    let result = call(
        driver,
        "clipboard_read",
        json!({ "include_text": true }),
        LIST_TIMEOUT,
    )
    .await?;
    let after = crate::computer::helper::clipboard::stamp(driver).await?;
    let read = result
        .structured
        .as_ref()
        .and_then(|s| s.get("text"))
        .and_then(Value::as_str)
        .map(|t| t.replace("\r\n", "\n"));
    let ours = read.as_deref() == Some(text.replace("\r\n", "\n").as_str());
    // Plain text and nothing else: the same text with HTML or an image
    // alongside is someone else's copy.
    if !ours || before != after || after.concealed || !after.plain_text {
        return Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            "The text was put on the clipboard, but something else was copied before it could \
             be checked, so nothing on it is held as yours: write it again before pasting.",
        ));
    }
    Ok(after.value)
}
