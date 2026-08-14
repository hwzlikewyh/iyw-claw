use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::error::{BrowserError, BrowserErrorCode};

const MAX_INPUT_BATCH: usize = 64;
const MAX_KEY_FIELD: usize = 128;
const MAX_TEXT_FIELD: usize = 4096;
const MAX_COORDINATE: f64 = 100_000.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserInputEvent {
    Mouse {
        #[serde(rename = "eventType")]
        event_type: String,
        x: f64,
        y: f64,
        #[serde(default = "default_button")]
        button: String,
        #[serde(default, rename = "clickCount")]
        click_count: u8,
        #[serde(default, rename = "deltaX")]
        delta_x: f64,
        #[serde(default, rename = "deltaY")]
        delta_y: f64,
        #[serde(default)]
        modifiers: u8,
    },
    Keyboard {
        #[serde(rename = "eventType")]
        event_type: String,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default, rename = "windowsVirtualKeyCode")]
        windows_virtual_key_code: u32,
        #[serde(default)]
        modifiers: u8,
    },
}

pub(super) fn validate_input_batch(
    events: &[BrowserInputEvent],
) -> Result<(Vec<Value>, bool), BrowserError> {
    if events.is_empty() || events.len() > MAX_INPUT_BATCH {
        return Err(invalid_input());
    }
    let mut semantic = false;
    let mut messages = Vec::with_capacity(events.len());
    for event in events {
        let (message, is_semantic) = event.to_wire()?;
        messages.push(message);
        semantic |= is_semantic;
    }
    Ok((messages, semantic))
}

impl BrowserInputEvent {
    fn to_wire(&self) -> Result<(Value, bool), BrowserError> {
        match self {
            Self::Mouse {
                event_type,
                x,
                y,
                button,
                click_count,
                delta_x,
                delta_y,
                modifiers,
            } => mouse_wire(MouseFields {
                event_type,
                x: *x,
                y: *y,
                button,
                click_count: *click_count,
                delta_x: *delta_x,
                delta_y: *delta_y,
                modifiers: *modifiers,
            }),
            Self::Keyboard {
                event_type,
                key,
                code,
                text,
                windows_virtual_key_code,
                modifiers,
            } => keyboard_wire(
                event_type,
                key.as_deref(),
                code.as_deref(),
                text.as_deref(),
                *windows_virtual_key_code,
                *modifiers,
            ),
        }
    }
}

struct MouseFields<'a> {
    event_type: &'a str,
    x: f64,
    y: f64,
    button: &'a str,
    click_count: u8,
    delta_x: f64,
    delta_y: f64,
    modifiers: u8,
}

fn mouse_wire(fields: MouseFields<'_>) -> Result<(Value, bool), BrowserError> {
    let allowed_type = matches!(
        fields.event_type,
        "mouseMoved" | "mousePressed" | "mouseReleased" | "mouseWheel"
    );
    let allowed_button = matches!(fields.button, "none" | "left" | "right" | "middle");
    let finite = [fields.x, fields.y, fields.delta_x, fields.delta_y]
        .iter()
        .all(|value| value.is_finite() && value.abs() <= MAX_COORDINATE);
    if !allowed_type
        || !allowed_button
        || !finite
        || fields.click_count > 3
        || fields.modifiers > 15
    {
        return Err(invalid_input());
    }
    Ok((
        json!({
            "type": "input_mouse",
            "eventType": fields.event_type,
            "x": fields.x,
            "y": fields.y,
            "button": fields.button,
            "clickCount": fields.click_count,
            "deltaX": fields.delta_x,
            "deltaY": fields.delta_y,
            "modifiers": fields.modifiers,
        }),
        fields.event_type != "mouseMoved",
    ))
}

fn keyboard_wire(
    event_type: &str,
    key: Option<&str>,
    code: Option<&str>,
    text: Option<&str>,
    virtual_key: u32,
    modifiers: u8,
) -> Result<(Value, bool), BrowserError> {
    if !matches!(event_type, "keyDown" | "rawKeyDown" | "keyUp" | "char")
        || modifiers > 15
        || !bounded(key, MAX_KEY_FIELD)
        || !bounded(code, MAX_KEY_FIELD)
        || !bounded(text, MAX_TEXT_FIELD)
    {
        return Err(invalid_input());
    }
    let mut message = serde_json::Map::new();
    message.insert("type".into(), json!("input_keyboard"));
    message.insert("eventType".into(), json!(event_type));
    message.insert("windowsVirtualKeyCode".into(), json!(virtual_key));
    message.insert("modifiers".into(), json!(modifiers));
    for (name, value) in [("key", key), ("code", code), ("text", text)] {
        if let Some(value) = value {
            message.insert(name.into(), json!(value));
        }
    }
    Ok((Value::Object(message), true))
}

fn bounded(value: Option<&str>, max: usize) -> bool {
    value.is_none_or(|value| value.len() <= max && !value.contains('\0'))
}

fn default_button() -> String {
    "none".to_string()
}

fn invalid_input() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The browser input batch is invalid",
    )
}
