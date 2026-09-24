// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! ETW event-record TDH decoder + property formatter.
//!
//! Turns raw `EVENT_RECORD` payloads into [`DecodedEventParts`] (a flat
//! `(name, value)` list) that the [`crate::extractors`] operate on. Only
//! the `InType`s we need for the learning-mode denial events are wired up;
//! the rest fall back to a textual placeholder so offset arithmetic stays
//! consistent without wasting cycles on unsupported encodings.

use std::collections::HashMap;
use std::fmt::Write as _;

use windows::core::GUID;
use windows::Win32::System::Diagnostics::Etw::{
    TdhGetEventInformation, EVENT_HEADER_EXT_TYPE_EVENT_SCHEMA_TL, EVENT_PROPERTY_INFO,
    EVENT_RECORD, TRACE_EVENT_INFO,
};

use crate::extractors::DecodedEventParts;

// TDH InType constants from evntrace.h / tdh.h.
const TDH_INTYPE_UNICODESTRING: u16 = 1;
const TDH_INTYPE_ANSISTRING: u16 = 2;
const TDH_INTYPE_INT8: u16 = 3;
const TDH_INTYPE_UINT8: u16 = 4;
const TDH_INTYPE_INT16: u16 = 5;
const TDH_INTYPE_UINT16: u16 = 6;
const TDH_INTYPE_INT32: u16 = 7;
const TDH_INTYPE_UINT32: u16 = 8;
const TDH_INTYPE_INT64: u16 = 9;
const TDH_INTYPE_UINT64: u16 = 10;
const TDH_INTYPE_FLOAT: u16 = 11;
const TDH_INTYPE_DOUBLE: u16 = 12;
const TDH_INTYPE_BOOLEAN: u16 = 13;
const TDH_INTYPE_BINARY: u16 = 14;
const TDH_INTYPE_GUID: u16 = 15;
const TDH_INTYPE_POINTER: u16 = 16;
const TDH_INTYPE_FILETIME: u16 = 17;
const TDH_INTYPE_SYSTEMTIME: u16 = 18;
const TDH_INTYPE_SID: u16 = 19;
const TDH_INTYPE_HEXINT32: u16 = 20;
const TDH_INTYPE_HEXINT64: u16 = 21;
const TDH_INTYPE_UNICODECHAR: u16 = 306;
const TDH_INTYPE_ANSICHAR: u16 = 307;
const TDH_INTYPE_SIZET: u16 = 308;
const PROPERTY_STRUCT: i32 = 0x1;
const PROPERTY_PARAM_LENGTH: i32 = 0x2;
const PROPERTY_PARAM_COUNT: i32 = 0x4;
const MAX_PROPERTY_ELEMENTS: usize = 4096;
const MAX_STRUCT_DEPTH: usize = 32;
const MAX_DECODE_WORK: usize = 100_000;
const MAX_SCHEMA_CACHE_ENTRIES: usize = 4096;
const EVENT_HEADER_FLAG_32_BIT_HEADER: u16 = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EventSchemaKey {
    provider: GUID,
    id: u16,
    version: u8,
    channel: u8,
    level: u8,
    opcode: u8,
    task: u16,
    keyword: u64,
}

impl EventSchemaKey {
    fn from_record(event_record: &EVENT_RECORD) -> Self {
        let header = event_record.EventHeader;
        let descriptor = header.EventDescriptor;
        Self {
            provider: header.ProviderId,
            id: descriptor.Id,
            version: descriptor.Version,
            channel: descriptor.Channel,
            level: descriptor.Level,
            opcode: descriptor.Opcode,
            task: descriptor.Task,
            keyword: descriptor.Keyword,
        }
    }
}

#[derive(Default)]
pub(crate) struct EventSchemaCache {
    schemas: HashMap<EventSchemaKey, TdhInfoBuffer>,
}

#[derive(Debug)]
pub(crate) enum DecodeError {
    Schema(String),
    Event {
        kind: EventDecodeKind,
        event_name: Option<String>,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventDecodeKind {
    PayloadMalformed,
    DecoderLimitReached,
    UnsupportedPropertyEncoding,
}

impl DecodeError {
    pub(crate) fn is_schema_error(&self) -> bool {
        matches!(self, Self::Schema(_))
    }

    pub(crate) fn event_kind(&self) -> Option<EventDecodeKind> {
        match self {
            Self::Event { kind, .. } => Some(*kind),
            Self::Schema(_) => None,
        }
    }

    pub(crate) fn event_name(&self) -> Option<&str> {
        match self {
            Self::Event { event_name, .. } => event_name.as_deref(),
            Self::Schema(_) => None,
        }
    }

    pub(crate) fn event(
        kind: EventDecodeKind,
        message: String,
        event_name: Option<String>,
    ) -> Self {
        Self::Event {
            kind,
            event_name,
            message,
        }
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schema(message) | Self::Event { message, .. } => f.write_str(message),
        }
    }
}

struct TdhInfoBuffer {
    storage: Vec<std::mem::MaybeUninit<TRACE_EVENT_INFO>>,
    len: usize,
}

impl TdhInfoBuffer {
    fn new(len: usize) -> Self {
        let element_size = std::mem::size_of::<TRACE_EVENT_INFO>();
        let element_count = len.div_ceil(element_size).max(1);
        let mut storage = Vec::with_capacity(element_count);
        storage.resize_with(element_count, std::mem::MaybeUninit::uninit);
        // SAFETY: `storage` owns `element_count` writable elements. Zeroing
        // them makes the complete byte view initialized before TDH fills it.
        unsafe {
            std::ptr::write_bytes(storage.as_mut_ptr(), 0, element_count);
        }
        Self { storage, len }
    }

    fn as_mut_ptr(&mut self) -> *mut TRACE_EVENT_INFO {
        self.storage.as_mut_ptr().cast()
    }

    fn as_ptr(&self) -> *const TRACE_EVENT_INFO {
        self.storage.as_ptr().cast()
    }

    fn as_bytes(&self) -> &[u8] {
        // SAFETY: `new` zero-initializes the allocation, and `len` never
        // exceeds its capacity in bytes. The storage alignment is that of
        // `TRACE_EVENT_INFO`, while this view is used only for byte offsets.
        unsafe { std::slice::from_raw_parts(self.storage.as_ptr().cast(), self.len) }
    }
}

/// Decodes an `EVENT_RECORD` into `DecodedEventParts`.
///
/// Returns `None` when TDH can't describe the event (rare — usually
/// indicates a corrupted or unknown event).
///
/// # Safety
/// `event_record` must point to a valid `EVENT_RECORD` provided by the
/// ETW callback; the caller must not retain references to its fields
/// after the callback returns.
pub unsafe fn decode_event_parts(
    event_record: *mut EVENT_RECORD,
    schema_cache: &mut EventSchemaCache,
) -> Result<DecodedEventParts, DecodeError> {
    let event = unsafe { &*event_record };
    let mut uncached_schema = None;
    let buffer = unsafe { event_schema(event_record, event, schema_cache, &mut uncached_schema) }?;
    let info = unsafe { &*buffer.as_ptr() };

    let header = event.EventHeader;
    let event_id = header.EventDescriptor.Id;
    let pointer_size = pointer_size_from_header_flags(header.Flags);
    let event_name = schema_event_name(buffer.as_bytes(), info);
    let props = decode_properties(buffer.as_bytes(), info, event_record, pointer_size)
        .map_err(|error| map_property_decode_error(error, event_name))?;

    Ok(DecodedEventParts {
        provider: header.ProviderId,
        event_id,
        props,
    })
}

/// Decodes one named property without decoding properties that follow it.
///
/// # Safety
/// `event_record` must satisfy the same requirements as [`decode_event_parts`].
pub unsafe fn decode_event_property(
    event_record: *mut EVENT_RECORD,
    schema_cache: &mut EventSchemaCache,
    property_name: &str,
) -> Result<Option<String>, DecodeError> {
    let event = unsafe { &*event_record };
    let mut uncached_schema = None;
    let buffer = unsafe { event_schema(event_record, event, schema_cache, &mut uncached_schema) }?;
    let info = unsafe { &*buffer.as_ptr() };
    let event_name = schema_event_name(buffer.as_bytes(), info);
    decode_named_property(
        buffer.as_bytes(),
        info,
        event_record,
        pointer_size_from_header_flags(event.EventHeader.Flags),
        property_name,
    )
    .map_err(|error| map_property_decode_error(error, event_name))
}

unsafe fn event_schema<'a>(
    event_record: *mut EVENT_RECORD,
    event: &EVENT_RECORD,
    schema_cache: &'a mut EventSchemaCache,
    uncached_schema: &'a mut Option<TdhInfoBuffer>,
) -> Result<&'a TdhInfoBuffer, DecodeError> {
    let key = EventSchemaKey::from_record(event);
    let cacheable = !unsafe { has_trace_logging_schema(event) };
    if !cacheable {
        *uncached_schema = Some(unsafe { load_event_schema(event_record) }?);
    } else if !schema_cache.schemas.contains_key(&key) {
        let schema = unsafe { load_event_schema(event_record) }?;
        cache_or_retain_schema(schema_cache, key, schema, uncached_schema);
    }
    schema_buffer(schema_cache, &key, uncached_schema)
}

fn cache_or_retain_schema(
    schema_cache: &mut EventSchemaCache,
    key: EventSchemaKey,
    schema: TdhInfoBuffer,
    uncached_schema: &mut Option<TdhInfoBuffer>,
) {
    if schema_cache.schemas.len() < MAX_SCHEMA_CACHE_ENTRIES {
        schema_cache.schemas.insert(key, schema);
    } else {
        *uncached_schema = Some(schema);
    }
}

fn schema_buffer<'a>(
    schema_cache: &'a EventSchemaCache,
    key: &EventSchemaKey,
    uncached_schema: &'a Option<TdhInfoBuffer>,
) -> Result<&'a TdhInfoBuffer, DecodeError> {
    if let Some(buffer) = uncached_schema {
        return Ok(buffer);
    }
    schema_cache
        .schemas
        .get(key)
        .ok_or_else(|| DecodeError::Schema("event schema cache lookup failed".to_string()))
}

fn map_property_decode_error(
    error: PropertyDecodeError,
    event_name: Option<String>,
) -> DecodeError {
    match error.kind {
        PropertyDecodeErrorKind::Schema => DecodeError::Schema(error.message),
        PropertyDecodeErrorKind::PayloadMalformed => {
            DecodeError::event(EventDecodeKind::PayloadMalformed, error.message, event_name)
        }
        PropertyDecodeErrorKind::DecoderLimitReached => DecodeError::event(
            EventDecodeKind::DecoderLimitReached,
            error.message,
            event_name,
        ),
        PropertyDecodeErrorKind::UnsupportedPropertyEncoding => DecodeError::event(
            EventDecodeKind::UnsupportedPropertyEncoding,
            error.message,
            event_name,
        ),
    }
}

fn schema_event_name(info_buf: &[u8], info: &TRACE_EVENT_INFO) -> Option<String> {
    // SAFETY: `info` points into the TDH buffer and this union member is the
    // event-name offset for the decoding sources used by these providers.
    let event_name_offset = unsafe { info.Anonymous1.EventNameOffset };
    wide_str_at(info_buf, event_name_offset).or_else(|| wide_str_at(info_buf, info.TaskNameOffset))
}

unsafe fn load_event_schema(event_record: *mut EVENT_RECORD) -> Result<TdhInfoBuffer, DecodeError> {
    let mut buf_size: u32 = 0;
    // First call: discover required buffer size. ERROR_INSUFFICIENT_BUFFER = 122.
    let status = unsafe { TdhGetEventInformation(event_record, None, None, &mut buf_size) };
    if status != 122 {
        return Err(DecodeError::Schema(format!(
            "TdhGetEventInformation(size) failed with Win32 error {status}"
        )));
    }

    let mut buffer = TdhInfoBuffer::new(buf_size as usize);
    let info_ptr = buffer.as_mut_ptr();
    let status =
        unsafe { TdhGetEventInformation(event_record, None, Some(info_ptr), &mut buf_size) };
    if status != 0 {
        return Err(DecodeError::Schema(format!(
            "TdhGetEventInformation(data) failed with Win32 error {status}"
        )));
    }

    Ok(buffer)
}

unsafe fn has_trace_logging_schema(event_record: &EVENT_RECORD) -> bool {
    if event_record.ExtendedData.is_null() || event_record.ExtendedDataCount == 0 {
        return false;
    }
    // SAFETY: ETW owns an array of `ExtendedDataCount` entries for the
    // callback lifetime.
    let items = unsafe {
        std::slice::from_raw_parts(
            event_record.ExtendedData,
            event_record.ExtendedDataCount as usize,
        )
    };
    items
        .iter()
        .any(|item| u32::from(item.ExtType) == EVENT_HEADER_EXT_TYPE_EVENT_SCHEMA_TL)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertyDecodeErrorKind {
    Schema,
    PayloadMalformed,
    DecoderLimitReached,
    UnsupportedPropertyEncoding,
}

#[derive(Debug)]
struct PropertyDecodeError {
    kind: PropertyDecodeErrorKind,
    message: String,
}

impl PropertyDecodeError {
    fn schema(message: String) -> Self {
        Self {
            kind: PropertyDecodeErrorKind::Schema,
            message,
        }
    }

    fn payload(message: String) -> Self {
        Self {
            kind: PropertyDecodeErrorKind::PayloadMalformed,
            message,
        }
    }

    fn limit(message: String) -> Self {
        Self {
            kind: PropertyDecodeErrorKind::DecoderLimitReached,
            message,
        }
    }

    fn unsupported(message: String) -> Self {
        Self {
            kind: PropertyDecodeErrorKind::UnsupportedPropertyEncoding,
            message,
        }
    }
}

fn decode_properties(
    info_buf: &[u8],
    info: &TRACE_EVENT_INFO,
    event_record: *mut EVENT_RECORD,
    pointer_size: usize,
) -> Result<Vec<(String, String)>, PropertyDecodeError> {
    // SAFETY: `decode_event_property` and the ETW callback supply a non-null
    // `EVENT_RECORD` that remains valid for this synchronous decode. This
    // borrow reads only its fixed-size header fields.
    let event = unsafe { &*event_record };
    let user_data = event.UserData as *const u8;
    let user_data_len = event.UserDataLength as usize;

    if user_data.is_null() || user_data_len == 0 {
        return Ok(Vec::new());
    }

    let property_count = info.PropertyCount as usize;
    let prop_count = info.TopLevelPropertyCount as usize;
    if prop_count > property_count {
        return Err(PropertyDecodeError::schema(format!(
            "top-level property count {prop_count} exceeds property count {property_count}"
        )));
    }

    let mut results = Vec::with_capacity(prop_count);
    let mut numeric_values = vec![None; property_count];
    let mut offset: usize = 0;
    let mut work_remaining = MAX_DECODE_WORK;
    let context = PropertyDecodeContext {
        info_buf,
        info,
        user_data,
        user_data_len,
        pointer_size,
    };

    for i in 0..prop_count {
        decode_property(
            i,
            &context,
            &mut offset,
            &mut numeric_values,
            0,
            &mut work_remaining,
            &mut results,
        )?;
    }

    Ok(results)
}

fn decode_named_property(
    info_buf: &[u8],
    info: &TRACE_EVENT_INFO,
    event_record: *mut EVENT_RECORD,
    pointer_size: usize,
    property_name: &str,
) -> Result<Option<String>, PropertyDecodeError> {
    // SAFETY: caller passes a valid EVENT_RECORD; the field accesses
    // are reads of POD fields.
    let event = unsafe { &*event_record };
    let user_data = event.UserData as *const u8;
    let user_data_len = event.UserDataLength as usize;
    if user_data.is_null() || user_data_len == 0 {
        return Ok(None);
    }

    let property_count = info.PropertyCount as usize;
    let prop_count = info.TopLevelPropertyCount as usize;
    if prop_count > property_count {
        return Err(PropertyDecodeError::schema(format!(
            "top-level property count {prop_count} exceeds property count {property_count}"
        )));
    }

    let mut numeric_values = vec![None; property_count];
    let mut offset = 0;
    let mut work_remaining = MAX_DECODE_WORK;
    let mut decoded = Vec::new();
    let context = PropertyDecodeContext {
        info_buf,
        info,
        user_data,
        user_data_len,
        pointer_size,
    };
    for index in 0..prop_count {
        decode_property(
            index,
            &context,
            &mut offset,
            &mut numeric_values,
            0,
            &mut work_remaining,
            &mut decoded,
        )?;
        if let Some((_, value)) = decoded
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(property_name))
        {
            return Ok(Some(value.clone()));
        }
        decoded.clear();
    }
    Ok(None)
}

struct PropertyDecodeContext<'a> {
    info_buf: &'a [u8],
    info: &'a TRACE_EVENT_INFO,
    user_data: *const u8,
    user_data_len: usize,
    pointer_size: usize,
}

fn decode_property(
    index: usize,
    context: &PropertyDecodeContext<'_>,
    offset: &mut usize,
    numeric_values: &mut [Option<usize>],
    depth: usize,
    work_remaining: &mut usize,
    results: &mut Vec<(String, String)>,
) -> Result<(), PropertyDecodeError> {
    *work_remaining = work_remaining.checked_sub(1).ok_or_else(|| {
        PropertyDecodeError::limit(format!(
            "property decode work exceeds limit {MAX_DECODE_WORK}"
        ))
    })?;
    if depth > MAX_STRUCT_DEPTH {
        return Err(PropertyDecodeError::limit(format!(
            "property nesting exceeds limit {MAX_STRUCT_DEPTH}"
        )));
    }
    if index >= context.info.PropertyCount as usize {
        return Err(PropertyDecodeError::schema(format!(
            "property index {index} is out of range"
        )));
    }
    let prop_info = event_property_info(context.info, index);
    let prop_name = wide_str_at(context.info_buf, prop_info.NameOffset)
        .unwrap_or_else(|| format!("prop{index}"));
    let flags = prop_info.Flags.0;
    let count = resolve_property_count(prop_info, flags, numeric_values, index)?;
    if count > MAX_PROPERTY_ELEMENTS {
        return Err(PropertyDecodeError::limit(format!(
            "property '{prop_name}' count {count} exceeds limit {MAX_PROPERTY_ELEMENTS}"
        )));
    }

    if flags & PROPERTY_STRUCT != 0 {
        let start_index = unsafe { prop_info.Anonymous1.structType.StructStartIndex } as usize;
        let member_count = unsafe { prop_info.Anonymous1.structType.NumOfStructMembers } as usize;
        let end_index = start_index.checked_add(member_count).ok_or_else(|| {
            PropertyDecodeError::schema(format!(
                "property '{prop_name}' struct member range overflows"
            ))
        })?;
        if end_index > context.info.PropertyCount as usize {
            return Err(PropertyDecodeError::schema(format!(
                "property '{prop_name}' struct member range {start_index}..{end_index} exceeds property count {}",
                context.info.PropertyCount
            )));
        }
        if member_count == 0 {
            return Err(PropertyDecodeError::schema(format!(
                "property '{prop_name}' has no struct members"
            )));
        }
        results.push((prop_name, "<struct>".to_string()));
        for _ in 0..count {
            for child_index in start_index..end_index {
                decode_property(
                    child_index,
                    context,
                    offset,
                    numeric_values,
                    depth + 1,
                    work_remaining,
                    results,
                )?;
            }
        }
        return Ok(());
    }

    let in_type = unsafe { prop_info.Anonymous1.nonStructType.InType };
    let declared_length = if flags & PROPERTY_PARAM_LENGTH != 0 {
        let length_index = unsafe { prop_info.Anonymous3.lengthPropertyIndex } as usize;
        resolved_metadata(numeric_values, length_index, "length", index)?
    } else {
        (unsafe { prop_info.Anonymous3.length }) as usize
    };

    let mut values = Vec::with_capacity(count.min(available_element_bound(
        in_type,
        context.user_data_len.saturating_sub(*offset),
        context.pointer_size,
    )));
    let mut numeric_value = None;
    for _ in 0..count {
        let remaining = context.user_data_len.saturating_sub(*offset);
        if remaining == 0 {
            return Err(PropertyDecodeError::payload(format!(
                "property '{prop_name}' exceeds the event payload"
            )));
        }
        let data_ptr = if remaining > 0 {
            unsafe { context.user_data.add(*offset) }
        } else {
            std::ptr::null()
        };
        let (value, consumed) = format_property_value_with_pointer_size(
            in_type,
            declared_length,
            data_ptr,
            remaining,
            context.pointer_size,
        );
        if consumed == 0 && remaining > 0 {
            return Err(zero_consumption_error(&prop_name, in_type));
        }

        *offset = offset
            .checked_add(consumed)
            .filter(|next| *next <= context.user_data_len)
            .ok_or_else(|| {
                PropertyDecodeError::payload(format!(
                    "property '{prop_name}' exceeds the event payload"
                ))
            })?;
        if count == 1 {
            numeric_value = parse_numeric_metadata(&value);
        }
        values.push(value);
    }
    numeric_values[index] = numeric_value;

    let rendered = if values.len() == 1 {
        values.pop().unwrap_or_default()
    } else {
        format!("[{}]", values.join(", "))
    };
    results.push((prop_name, rendered));
    Ok(())
}

fn resolve_property_count(
    prop_info: &EVENT_PROPERTY_INFO,
    flags: i32,
    numeric_values: &[Option<usize>],
    property_index: usize,
) -> Result<usize, PropertyDecodeError> {
    if flags & PROPERTY_PARAM_COUNT != 0 {
        let count_index = unsafe { prop_info.Anonymous2.countPropertyIndex } as usize;
        resolved_metadata(numeric_values, count_index, "count", property_index)
    } else {
        let count = unsafe { prop_info.Anonymous2.count } as usize;
        Ok(count.max(1))
    }
}

fn pointer_size_from_header_flags(flags: u16) -> usize {
    if flags & EVENT_HEADER_FLAG_32_BIT_HEADER != 0 {
        4
    } else {
        8
    }
}

fn available_element_bound(in_type: u16, available: usize, pointer_size: usize) -> usize {
    let element_size = match in_type {
        TDH_INTYPE_INT8 | TDH_INTYPE_UINT8 => 1,
        TDH_INTYPE_INT16 | TDH_INTYPE_UINT16 => 2,
        TDH_INTYPE_INT32 | TDH_INTYPE_UINT32 | TDH_INTYPE_BOOLEAN | TDH_INTYPE_HEXINT32 => 4,
        TDH_INTYPE_POINTER => pointer_size,
        TDH_INTYPE_INT64 | TDH_INTYPE_UINT64 | TDH_INTYPE_HEXINT64 => 8,
        _ => 1,
    };
    (available / element_size).max(1)
}

fn event_property_info(info: &TRACE_EVENT_INFO, index: usize) -> &EVENT_PROPERTY_INFO {
    unsafe {
        let base = std::ptr::addr_of!(info.EventPropertyInfoArray) as *const EVENT_PROPERTY_INFO;
        &*base.add(index)
    }
}

fn resolved_metadata(
    numeric_values: &[Option<usize>],
    metadata_index: usize,
    kind: &str,
    property_index: usize,
) -> Result<usize, PropertyDecodeError> {
    let Some(value) = numeric_values.get(metadata_index) else {
        return Err(PropertyDecodeError::schema(format!(
            "property {property_index} references out-of-range {kind} property {metadata_index}"
        )));
    };
    value.ok_or_else(|| {
        PropertyDecodeError::schema(format!(
            "property {property_index} references unresolved {kind} property {metadata_index}"
        ))
    })
}

fn is_known_property_type(in_type: u16) -> bool {
    matches!(
        in_type,
        TDH_INTYPE_UNICODESTRING
            | TDH_INTYPE_ANSISTRING
            | TDH_INTYPE_INT8
            | TDH_INTYPE_UINT8
            | TDH_INTYPE_INT16
            | TDH_INTYPE_UINT16
            | TDH_INTYPE_INT32
            | TDH_INTYPE_UINT32
            | TDH_INTYPE_INT64
            | TDH_INTYPE_UINT64
            | TDH_INTYPE_FLOAT
            | TDH_INTYPE_DOUBLE
            | TDH_INTYPE_BOOLEAN
            | TDH_INTYPE_BINARY
            | TDH_INTYPE_GUID
            | TDH_INTYPE_POINTER
            | TDH_INTYPE_FILETIME
            | TDH_INTYPE_SYSTEMTIME
            | TDH_INTYPE_SID
            | TDH_INTYPE_HEXINT32
            | TDH_INTYPE_HEXINT64
            | TDH_INTYPE_UNICODECHAR
            | TDH_INTYPE_ANSICHAR
            | TDH_INTYPE_SIZET
    )
}

fn zero_consumption_error(property_name: &str, in_type: u16) -> PropertyDecodeError {
    let message = format!("property '{property_name}' could not be decoded");
    if is_known_property_type(in_type) {
        PropertyDecodeError::payload(message)
    } else {
        PropertyDecodeError::unsupported(message)
    }
}

fn parse_numeric_metadata(value: &str) -> Option<usize> {
    let value = value.trim().trim_matches('"');
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .and_then(|hex| usize::from_str_radix(hex, 16).ok())
        .or_else(|| value.parse().ok())
}

fn format_property_value_with_pointer_size(
    in_type: u16,
    declared_length: usize,
    data: *const u8,
    available: usize,
    pointer_size: usize,
) -> (String, usize) {
    if data.is_null() || available == 0 {
        return ("<no data>".to_string(), 0);
    }

    match in_type {
        TDH_INTYPE_UNICODESTRING => {
            if declared_length > available || !declared_length.is_multiple_of(2) {
                return ("<truncated unicode string>".to_string(), 0);
            }
            let byte_len = if declared_length > 0 {
                declared_length
            } else {
                available
            };
            let max_wchars = byte_len / 2;
            // SAFETY: data is valid for `available` bytes; byte_len is bounded
            // by available. Decode from byte pairs because ETW does not
            // guarantee payload alignment.
            let bytes = unsafe { std::slice::from_raw_parts(data, byte_len) };
            let mut wchars = Vec::with_capacity(max_wchars);
            let mut terminator_index = None;
            for (index, chunk) in bytes.chunks_exact(2).enumerate() {
                let wchar = u16::from_le_bytes([chunk[0], chunk[1]]);
                if wchar == 0 {
                    terminator_index = Some(index);
                    break;
                }
                wchars.push(wchar);
            }
            let s = String::from_utf16_lossy(&wchars);
            // Include the null terminator in consumed bytes when present.
            let consumed = if declared_length > 0 {
                byte_len
            } else {
                let Some(index) = terminator_index else {
                    return ("<truncated unicode string>".to_string(), 0);
                };
                (index + 1) * 2
            };
            (format!("\"{s}\""), consumed)
        }
        TDH_INTYPE_ANSISTRING => {
            if declared_length > available {
                return ("<truncated ansi string>".to_string(), 0);
            }
            let byte_len = if declared_length > 0 {
                declared_length
            } else {
                available
            };
            let bytes = unsafe { std::slice::from_raw_parts(data, byte_len) };
            let terminator = bytes.iter().position(|&b| b == 0);
            if declared_length == 0 && terminator.is_none() {
                return ("<truncated ansi string>".to_string(), 0);
            }
            let len = terminator.unwrap_or(byte_len);
            let s = String::from_utf8_lossy(&bytes[..len]);
            let consumed = if declared_length > 0 {
                byte_len
            } else {
                (len + 1).min(available)
            };
            (format!("\"{s}\""), consumed)
        }
        TDH_INTYPE_INT8 if available >= 1 => {
            let v = unsafe { *data } as i8;
            (v.to_string(), 1)
        }
        TDH_INTYPE_UINT8 if available >= 1 => {
            let v = unsafe { *data };
            (v.to_string(), 1)
        }
        TDH_INTYPE_BOOLEAN if available >= 4 => {
            // SAFETY: data points to >=4 valid bytes; read_unaligned because
            // ETW payload alignment is not guaranteed.
            let v = unsafe { (data.cast::<u32>()).read_unaligned() };
            (if v != 0 { "true" } else { "false" }.to_string(), 4)
        }
        TDH_INTYPE_INT16 if available >= 2 => (
            unsafe { (data.cast::<i16>()).read_unaligned() }.to_string(),
            2,
        ),
        TDH_INTYPE_UINT16 if available >= 2 => (
            unsafe { (data.cast::<u16>()).read_unaligned() }.to_string(),
            2,
        ),
        TDH_INTYPE_INT32 if available >= 4 => (
            unsafe { (data.cast::<i32>()).read_unaligned() }.to_string(),
            4,
        ),
        TDH_INTYPE_UINT32 if available >= 4 => (
            unsafe { (data.cast::<u32>()).read_unaligned() }.to_string(),
            4,
        ),
        TDH_INTYPE_HEXINT32 if available >= 4 => (
            format!("{:#x}", unsafe { (data.cast::<u32>()).read_unaligned() }),
            4,
        ),
        TDH_INTYPE_INT64 if available >= 8 => (
            unsafe { (data.cast::<i64>()).read_unaligned() }.to_string(),
            8,
        ),
        TDH_INTYPE_UINT64 if available >= 8 => (
            unsafe { (data.cast::<u64>()).read_unaligned() }.to_string(),
            8,
        ),
        TDH_INTYPE_HEXINT64 if available >= 8 => (
            format!("{:#x}", unsafe { (data.cast::<u64>()).read_unaligned() }),
            8,
        ),
        TDH_INTYPE_POINTER if pointer_size == 4 && available >= 4 => (
            format!("{:#x}", unsafe { (data.cast::<u32>()).read_unaligned() }),
            4,
        ),
        TDH_INTYPE_POINTER if pointer_size == 8 && available >= 8 => (
            format!("{:#x}", unsafe { (data.cast::<u64>()).read_unaligned() }),
            8,
        ),
        TDH_INTYPE_SID if available >= 8 => format_sid(data, available),
        TDH_INTYPE_BINARY if declared_length > 0 && declared_length <= available => {
            let bytes = unsafe { std::slice::from_raw_parts(data, declared_length) };
            let mut rendered = String::with_capacity(4 + declared_length * 2);
            rendered.push_str("hex:");
            for byte in bytes {
                let _ = write!(rendered, "{byte:02X}");
            }
            (rendered, declared_length)
        }
        // Known-but-unformatted fixed-width values still have an implicit
        // payload size when TDH reports a zero declared length.
        _ => {
            let length = if declared_length > 0 {
                Some(declared_length)
            } else {
                implicit_fixed_width(in_type, pointer_size)
            };
            match length {
                Some(length) if length <= available => ("<unsupported>".to_string(), length),
                _ => ("<unsupported>".to_string(), 0),
            }
        }
    }
}

fn implicit_fixed_width(in_type: u16, pointer_size: usize) -> Option<usize> {
    match in_type {
        TDH_INTYPE_INT8 | TDH_INTYPE_UINT8 | TDH_INTYPE_ANSICHAR => Some(1),
        TDH_INTYPE_INT16 | TDH_INTYPE_UINT16 | TDH_INTYPE_UNICODECHAR => Some(2),
        TDH_INTYPE_INT32 | TDH_INTYPE_UINT32 | TDH_INTYPE_FLOAT | TDH_INTYPE_BOOLEAN
        | TDH_INTYPE_HEXINT32 => Some(4),
        TDH_INTYPE_INT64 | TDH_INTYPE_UINT64 | TDH_INTYPE_DOUBLE | TDH_INTYPE_FILETIME
        | TDH_INTYPE_HEXINT64 => Some(8),
        TDH_INTYPE_GUID | TDH_INTYPE_SYSTEMTIME => Some(16),
        TDH_INTYPE_POINTER | TDH_INTYPE_SIZET => Some(pointer_size),
        _ => None,
    }
}

fn format_sid(data: *const u8, available: usize) -> (String, usize) {
    let header = unsafe { std::slice::from_raw_parts(data, available.min(8)) };
    if header.len() < 8 {
        return ("<invalid SID>".to_string(), 0);
    }
    let sub_authority_count = header[1] as usize;
    let length = 8usize.saturating_add(sub_authority_count.saturating_mul(4));
    if length > available {
        return ("<invalid SID>".to_string(), 0);
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, length) };
    let authority = bytes[2..8]
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte));
    let mut sid = format!("S-{}-{authority}", bytes[0]);
    for index in 0..sub_authority_count {
        let start = 8 + index * 4;
        let value = u32::from_le_bytes([
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
        ]);
        sid.push_str(&format!("-{value}"));
    }
    (sid, length)
}

fn wide_str_at(buf: &[u8], offset: u32) -> Option<String> {
    let offset = offset as usize;
    if offset == 0 || offset >= buf.len() {
        return None;
    }
    let slice = &buf[offset..];
    // The buffer is u8-aligned but the names are u16-aligned by
    // construction (TDH places them at even offsets). Iterate u16
    // pairs until null terminator or end of buffer.
    let mut end = slice.len();
    let mut i = 0;
    while i + 1 < slice.len() {
        let lo = slice[i] as u16;
        let hi = slice[i + 1] as u16;
        let wchar = lo | (hi << 8);
        if wchar == 0 {
            end = i;
            break;
        }
        i += 2;
    }
    let trimmed = &slice[..end];
    // Build a Vec<u16> from byte pairs.
    let wchars: Vec<u16> = trimmed
        .chunks_exact(2)
        .map(|p| (p[0] as u16) | ((p[1] as u16) << 8))
        .collect();
    if wchars.is_empty() {
        None
    } else {
        Some(String::from_utf16_lossy(&wchars))
    }
}
