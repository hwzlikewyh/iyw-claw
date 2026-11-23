use super::error::GatewayError;
use super::json_scan::{ImageFormat, ImageSlot, Scan, Value};
use super::limits::MAX_IMAGES;

impl Value {
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key)?.text.as_deref()
    }
}

pub(super) fn container(key: &str) -> bool {
    matches!(
        key,
        "input"
            | "messages"
            | "system"
            | "content"
            | "output"
            | "source"
            | "image_url"
            | "response"
    )
}

pub(super) fn slot(object: &Value, editing: bool, prepare: bool) -> Option<ImageSlot> {
    let format = match object.text("type") {
        _ if editing => ImageFormat::Edit,
        Some("input_image") => ImageFormat::Responses,
        Some("image_url") => ImageFormat::Chat,
        Some("image") => ImageFormat::Messages,
        _ if prepare && object.text("operation") == Some("upload") => ImageFormat::Responses,
        _ => return None,
    };
    let value = image_value(object, format, prepare)?;
    if value.span.end < value.span.start + 2 {
        return None;
    }
    let raw = !editing
        && object
            .get("source")
            .is_some_and(|source| source.text("type") == Some("base64"))
        || prepare;
    let managed_id = value
        .text
        .as_deref()
        .filter(|value| managed(value))
        .map(|value| value.trim_start_matches("iyw-image:").to_owned());
    if !value.inline && !raw && managed_id.is_none() {
        return None;
    }
    Some(image_slot(object, value, (format, managed_id)))
}

fn image_slot(
    object: &Value,
    value: &Value,
    (format, managed_id): (ImageFormat, Option<String>),
) -> ImageSlot {
    let source = if matches!(format, ImageFormat::Edit) {
        None
    } else {
        object.get("source")
    };
    ImageSlot {
        encoded: value.span.clone(),
        replace: source
            .map(|source| source.span.clone())
            .unwrap_or_else(|| value.span.clone()),
        mime: declared_mime(object),
        format,
        source_extra: source
            .map(|source| source.source_extra.clone())
            .unwrap_or_default(),
        rename: if matches!(format, ImageFormat::Responses) {
            object
                .get("file_id")
                .and_then(|value| value.key_span.clone())
        } else {
            None
        },
        managed_id,
    }
}

fn image_value(object: &Value, format: ImageFormat, prepare: bool) -> Option<&Value> {
    if prepare {
        return object.get("data");
    }
    match format {
        ImageFormat::Messages => {
            let source = object.get("source")?;
            if source.text("type") == Some("base64") {
                source.get("data")
            } else {
                source.get("url").or_else(|| source.get("file_id"))
            }
        }
        ImageFormat::Responses if object.get("file_id").is_some() => {
            if object.get("image_url").is_some() {
                None
            } else {
                object.get("file_id")
            }
        }
        ImageFormat::Edit => object.get("image_url").or_else(|| object.get("url")),
        _ => object
            .get("image_url")
            .map(|url| url.get("url").unwrap_or(url)),
    }
}

pub(super) fn managed(value: &str) -> bool {
    value.starts_with("iyw-image:iyw-image-v1-") || value.starts_with("iyw-image-v1-")
}

impl Scan {
    pub(super) fn record_edit_input(
        &mut self,
        item: &Value,
        mask: bool,
    ) -> Result<(), GatewayError> {
        let value = image_value(item, ImageFormat::Edit, false).unwrap_or(item);
        if !(value.https || value.inline || value.text.as_deref().is_some_and(managed)) {
            return Ok(());
        }
        self.has_edit_sources |= !mask;
        if item.get("image_url").is_some() {
            return Ok(());
        }
        // 对旧格式只记录字段边界，保留签名 URL 原文以及对象中的其他参数。
        const MAX_EDIT_CHANGES: usize = (MAX_IMAGES + 1) * 2;
        if self.edit_fields.len() + 2 > MAX_EDIT_CHANGES {
            return Err(GatewayError::input("Too many image edit fields"));
        }
        if let Some(key) = &value.key_span {
            self.edit_fields.push((key.clone(), br#""image_url""#));
        } else {
            let span = &value.span;
            self.edit_fields
                .push((span.start..span.start, br#"{"image_url":"#));
            self.edit_fields.push((span.end..span.end, b"}"));
        }
        Ok(())
    }
}

pub(super) fn string_slot(value: &Value, key: &str, editing: bool) -> Option<ImageSlot> {
    let managed_id = value
        .text
        .as_deref()
        .filter(|value| managed(value))
        .map(|value| value.trim_start_matches("iyw-image:").to_owned());
    if !editing || !matches!(key, "images" | "mask") || !value.inline && managed_id.is_none() {
        return None;
    }
    Some(ImageSlot {
        encoded: value.span.clone(),
        replace: value.span.clone(),
        mime: None,
        format: ImageFormat::Edit,
        source_extra: Vec::new(),
        managed_id,
        rename: None,
    })
}

fn declared_mime(object: &Value) -> Option<String> {
    object
        .get("source")
        .and_then(|source| source.text("media_type"))
        .or_else(|| object.text("mime_type"))
        .map(str::to_owned)
}
