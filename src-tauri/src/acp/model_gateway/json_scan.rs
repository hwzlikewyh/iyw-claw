use std::io::{BufRead, BufReader, Read};
use std::ops::Range;

use super::{
    error::GatewayError,
    limits::{IO_BUFFER_BYTES, MAX_IMAGES, MAX_JSON_DEPTH},
};

const MAX_TOKEN_BYTES: usize = 1024;
const MAX_OBJECT_FIELDS: usize = 256;

#[derive(Clone, Copy)]
pub(super) enum ImageFormat {
    Responses,
    Chat,
    Messages,
    Edit,
}

pub(super) struct ImageSlot {
    pub encoded: Range<u64>,
    pub replace: Range<u64>,
    pub mime: Option<String>,
    pub format: ImageFormat,
    pub source_extra: Vec<Range<u64>>,
}

#[derive(Default)]
pub(super) struct Scan {
    pub model: String,
    pub operation: String,
    pub images: Vec<ImageSlot>,
}

#[derive(Default)]
pub(super) struct Value {
    pub span: Range<u64>,
    pub text: Option<String>,
    pub fields: Vec<(String, Value)>,
    pub inline: bool,
    pub source_extra: Vec<Range<u64>>,
}

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

pub(super) struct Parser {
    pub input: BufReader<Box<dyn Read + Send>>,
    pub offset: u64,
    result: Scan,
    editing: bool,
    prepare: bool,
}

pub(super) fn scan(reader: Box<dyn Read + Send>, path: &str) -> Result<Scan, GatewayError> {
    let mut parser = Parser {
        input: BufReader::with_capacity(IO_BUFFER_BYTES, reader),
        offset: 0,
        result: Scan::default(),
        editing: path.ends_with("/images/edits"),
        prepare: path.ends_with("/images/input/prepare"),
    };
    parser.value(0, "root", true)?;
    parser.whitespace()?;
    if parser.peek()?.is_some() {
        return Err(GatewayError::input("Invalid JSON request"));
    }
    Ok(parser.result)
}

impl Parser {
    fn peek(&mut self) -> Result<Option<u8>, GatewayError> {
        Ok(self.input.fill_buf()?.first().copied())
    }

    pub(super) fn byte(&mut self) -> Result<u8, GatewayError> {
        let byte = self
            .peek()?
            .ok_or_else(|| GatewayError::input("Incomplete JSON request"))?;
        self.input.consume(1);
        self.offset += 1;
        Ok(byte)
    }

    fn whitespace(&mut self) -> Result<(), GatewayError> {
        while self.peek()?.is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.byte()?;
        }
        Ok(())
    }

    fn value(&mut self, depth: usize, key: &str, active: bool) -> Result<Value, GatewayError> {
        if depth > MAX_JSON_DEPTH {
            return Err(GatewayError::input(
                "JSON nesting exceeds the gateway limit",
            ));
        }
        self.whitespace()?;
        match self.peek()? {
            Some(b'"') => self.string_value(key, active),
            Some(b'{') => self.object(depth, key, active),
            Some(b'[') => self.array(depth, key, active),
            Some(_) => self.scalar(),
            None => Err(GatewayError::input("Empty JSON request")),
        }
    }

    fn string_value(&mut self, key: &str, active: bool) -> Result<Value, GatewayError> {
        let value = self.string()?;
        if active && self.editing && matches!(key, "images" | "mask") && value.inline {
            self.add_image(ImageSlot {
                encoded: value.span.clone(),
                replace: value.span.clone(),
                mime: None,
                format: ImageFormat::Edit,
                source_extra: Vec::new(),
            })?;
        }
        Ok(value)
    }

    fn array(&mut self, depth: usize, key: &str, active: bool) -> Result<Value, GatewayError> {
        self.byte()?;
        self.whitespace()?;
        if self.peek()? == Some(b']') {
            self.byte()?;
            return Ok(Value::default());
        }
        loop {
            self.value(depth + 1, key, active)?;
            self.whitespace()?;
            match self.byte()? {
                b']' => break,
                b',' => {}
                _ => return Err(GatewayError::input("Invalid JSON array")),
            }
        }
        Ok(Value::default())
    }

    fn scalar(&mut self) -> Result<Value, GatewayError> {
        let mut bytes = Vec::new();
        while self
            .peek()?
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}'))
        {
            if bytes.len() == MAX_TOKEN_BYTES {
                return Err(GatewayError::input("JSON scalar is too large"));
            }
            bytes.push(self.byte()?);
        }
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|_| GatewayError::input("Invalid JSON scalar"))?;
        Ok(Value::default())
    }

    fn object(&mut self, depth: usize, parent: &str, active: bool) -> Result<Value, GatewayError> {
        let start = self.offset;
        let first_image = self.result.images.len();
        self.byte()?;
        let mut object = Value::default();
        let mut fields = 0;
        self.whitespace()?;
        if self.peek()? == Some(b'}') {
            self.byte()?;
            return Ok(object);
        }
        loop {
            self.field(&mut object, (depth, active, parent == "source"))?;
            fields += 1;
            if fields > MAX_OBJECT_FIELDS {
                return Err(GatewayError::input("Too many JSON image fields"));
            }
            self.whitespace()?;
            match self.byte()? {
                b'}' => break,
                b',' => {}
                _ => return Err(GatewayError::input("Invalid JSON object separator")),
            }
        }
        object.span = start..self.offset;
        self.record_object(&object, (depth, parent, active), first_image)?;
        if !matches!(parent, "source" | "image_url") {
            object.fields.clear();
        }
        Ok(object)
    }

    fn record_object(
        &mut self,
        object: &Value,
        (depth, parent, active): (usize, &str, bool),
        first_image: usize,
    ) -> Result<(), GatewayError> {
        if matches!(object.text("type"), Some("tool_use" | "custom_tool_use")) {
            self.result.images.truncate(first_image);
        }
        if depth == 0 || parent == "response" && active {
            if let Some(model) = object.text("model") {
                self.result.model = model.to_owned();
            }
            self.result.operation = object.text("operation").unwrap_or_default().to_owned();
        }
        if active || self.prepare && depth == 0 {
            let editing = self.editing && matches!(parent, "images" | "mask");
            if let Some(slot) =
                super::json_image::slot(&object, editing, self.prepare && depth == 0)
            {
                self.add_image(slot)?;
            }
        }
        Ok(())
    }

    fn add_image(&mut self, slot: ImageSlot) -> Result<(), GatewayError> {
        if self.result.images.len() >= MAX_IMAGES {
            return Err(GatewayError::input("Too many request images"));
        }
        self.result.images.push(slot);
        Ok(())
    }

    fn field(
        &mut self,
        object: &mut Value,
        (depth, active, source): (usize, bool, bool),
    ) -> Result<(), GatewayError> {
        self.whitespace()?;
        let field_start = self.offset;
        if self.peek()? != Some(b'"') {
            return Err(GatewayError::input("Invalid JSON object key"));
        }
        let key = self
            .string()?
            .text
            .ok_or_else(|| GatewayError::input("JSON key exceeds limit"))?;
        self.whitespace()?;
        if self.byte()? != b':' {
            return Err(GatewayError::input("Invalid JSON object"));
        }
        let child_active = active && image_container(&key)
            || depth == 0 && self.editing && matches!(key.as_str(), "images" | "mask");
        let value = self.value(depth + 1, &key, child_active)?;
        if source && !matches!(key.as_str(), "type" | "data" | "media_type" | "url") {
            object.source_extra.push(field_start..self.offset);
        }
        if [
            "type",
            "mime_type",
            "media_type",
            "model",
            "operation",
            "data",
            "url",
            "image_url",
            "source",
        ]
        .contains(&key.as_str())
        {
            if object.get(&key).is_some() {
                return Err(GatewayError::input("Duplicate image metadata field"));
            }
            object.fields.push((key, value));
        }
        Ok(())
    }
}

fn image_container(key: &str) -> bool {
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
