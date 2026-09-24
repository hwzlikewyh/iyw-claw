// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! ETW event -> [`RawDenial`] extractors.
//!
//! These operate on the generic [`DecodedEventParts`] shape produced by
//! [`crate::tdh_decode`], so they can be unit-tested with hand-built
//! fixtures without a live/sealed ETW trace. A [`RawDenial`] still carries
//! the *kernel-form* object path; [`crate::etl_decode`] path-normalises and
//! de-duplicates them into the public
//! [`learning_mode_core::DeniedResource`].
//!
//! These policy-oriented extractors intentionally do not reuse the diagnostic
//! console's display mapping. The console accepts broad real-time provider
//! traffic and formats it for humans; this module accepts only the
//! learning-mode providers and produces the stable cross-platform denial
//! model used for policy generation.
//!
//! ## Event vocabulary
//!
//! The learning-mode ETL carries a set of event IDs that map onto the
//! resource types we surface. This list grows as more denial sources are
//! decoded; event IDs outside this vocabulary are excluded rather than
//! extracted, and (for the known providers below) that exclusion is
//! aggregated into [`learning_mode_core::VerboseLoggingSummary`] rather than
//! silently dropped. The IDs handled today:
//!
//! - **14 / 4907 — access check** — the primary denial event
//!   (`ObjectType` / `ObjectName` / `AccessMask`). `ObjectType` selects the
//!   resource type: `File` → [`ResourceType::File`], `Key` →
//!   [`ResourceType::Other`] (registry), and an **empty** `ObjectType` is a
//!   brokered-capability check → [`ResourceType::Capability`]. Only positively
//!   classified registry reads are actionable; writes and unknown registry
//!   access are retained only as verbose diagnostics. Named Section,
//!   SymbolicLink, and Timer objects are likewise verbose-only because MXC has
//!   no corresponding policy grants.
//!   Other object types are dropped until their access-mask vocabulary is
//!   understood. The [`AccessType`] is derived from the
//!   `AccessMask` field (see [`access_type_from_mask`]). Emitted under both
//!   learning modes (`block` → `Mode="Normal"`, `allow` →
//!   `Mode="Permissive"`).
//! - **27 — `LearningModeViolation`** — UI-surface denials →
//!   [`ResourceType::Ui`]. Carries no usable access mask, so the access type
//!   stays [`AccessType::Unknown`].
//! - **28 — schema-dependent denial** — either a UI violation carrying
//!   `Category` / `Detail` / `Denied`, or a compact capability-access-manager
//!   record (`Denied` / `PackageSid` / `ProcessId`). UI violations map to
//!   [`ResourceType::Ui`]. Capability records map to
//!   [`ResourceType::Capability`], with the capability name resolved from the
//!   capability SID via [`crate::capability_names`] (well-known SID → friendly
//!   name; custom hashed capabilities fall back to the SID string).

use learning_mode_core::{
    AccessType, ResourceType, VerboseLoggingOutcomeReason, VerboseLoggingProvider,
};
use sha2::{Digest, Sha256};
use windows::core::GUID;

/// Microsoft-Windows-Kernel-General provider.
pub(crate) const KERNEL_GENERAL_PROVIDER: GUID = GUID {
    data1: 0xa68c_a8b7,
    data2: 0x004f,
    data3: 0xd7b6,
    data4: [0xa6, 0x98, 0x07, 0xe2, 0xde, 0x0f, 0x1f, 0x5d],
};

/// Microsoft-Windows-Privacy-Auditing-PermissiveLearningMode provider.
pub(crate) const PRIVACY_LEARNING_MODE_PROVIDER: GUID = GUID {
    data1: 0x811a_1ddb,
    data2: 0x2e69,
    data3: 0x5f25,
    data4: [0xad, 0xc0, 0x4b, 0x18, 0x61, 0x70, 0xe7, 0x60],
};

pub(crate) const ACCESS_CHECK_EVENT_ID: u16 = 14;
pub(crate) const LEARNING_MODE_VIOLATION_EVENT_ID: u16 = 27;
pub(crate) const CAPABILITY_DENIAL_EVENT_ID: u16 = 28;
pub(crate) const PRIVACY_ACCESS_CHECK_EVENT_ID: u16 = 4907;

/// Pre-decoded event payload handed to the extractors.
///
/// The trace consumer decodes each raw `EVENT_RECORD` into this shape via
/// [`crate::tdh_decode::decode_event_parts`] before routing it here. The
/// extractors take only this representation so they stay unit-testable.
#[derive(Debug, Clone)]
pub struct DecodedEventParts {
    /// Provider that emitted the event. Event IDs are provider-scoped.
    pub provider: GUID,
    /// Originating ETW event ID.
    pub event_id: u16,
    /// `(name, value)` pairs from the decoded payload. String values are
    /// often TDH-quoted; extractors trim the surrounding quotes.
    pub props: Vec<(String, String)>,
}

/// A denial extracted from one ETW event, before path normalisation and
/// de-duplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDenial {
    /// Process ID that triggered the denial.
    pub pid: u32,
    /// Classified resource type.
    pub resource_type: ResourceType,
    /// Object name in kernel form (e.g. `\Device\HarddiskVolumeN\...`) or,
    /// for non-file resources, the raw identifier the event carried. Empty
    /// when the event carries no resolvable name (e.g. a capability denial
    /// whose name is still encoded in an undecoded SID blob).
    pub object_name: String,
    /// Access the workload was attempting.
    pub access_type: AccessType,
    /// Kernel `FILETIME` of the event.
    pub filetime: u64,
    /// Originating ETW event ID (kept for diagnostics).
    pub event_id: u16,
    /// Symbolic category of the originating provider, for verbose logging
    /// aggregation. Never a raw provider GUID.
    pub provider: VerboseLoggingProvider,
    /// Bounded sensitive-value-redacted properties retained for verbose logging signatures.
    pub verbose_logging_properties: Vec<(String, String)>,
}

/// Routes a decoded event to the matching extractor by its event ID.
///
/// Returns `Err` with a closed [`VerboseLoggingOutcomeReason`] for events that
/// are not learning-mode denials, that carry an object type we don't
/// surface, or that otherwise fail extraction. Callers aggregate the
/// returned reason into [`learning_mode_core::VerboseLoggingSummary`] rather than
/// discarding it, except when a fallback extractor (e.g.
/// [`crate::capability_dacl`]) recovers an equivalent denial from the same
/// event.
pub fn extract_denial(
    parts: &DecodedEventParts,
    pid: u32,
    filetime: u64,
) -> Result<RawDenial, VerboseLoggingOutcomeReason> {
    // Callers on the real trace path gate on `is_learning_mode_event` before
    // decoding via TDH at all, so this branch only matters for direct/test
    // callers that skip that gate.
    let provider = verbose_logging_provider_for_guid(parts.provider)
        .ok_or(VerboseLoggingOutcomeReason::UnsupportedEventSchema)?;
    if !is_learning_mode_event(parts.provider, parts.event_id) {
        return Err(VerboseLoggingOutcomeReason::UnsupportedEventSchema);
    }

    match parts.event_id {
        ACCESS_CHECK_EVENT_ID | PRIVACY_ACCESS_CHECK_EVENT_ID => {
            build_denial_from_access_check(parts, pid, filetime, provider)
        }
        LEARNING_MODE_VIOLATION_EVENT_ID => {
            build_denial_from_learning_mode(parts, pid, filetime, provider)
        }
        CAPABILITY_DENIAL_EVENT_ID if is_ui_violation_schema(parts) => {
            build_denial_from_learning_mode(parts, pid, filetime, provider)
        }
        CAPABILITY_DENIAL_EVENT_ID => build_denial_from_capability(parts, pid, filetime, provider),
        _ => Err(VerboseLoggingOutcomeReason::UnsupportedEventSchema),
    }
}

/// Returns typed denial classifications that can be determined independently
/// of whether the event contains every property required for actionable output.
pub(crate) fn verbose_logging_classification(
    parts: &DecodedEventParts,
) -> (Option<AccessType>, Option<ResourceType>) {
    match parts.event_id {
        ACCESS_CHECK_EVENT_ID | PRIVACY_ACCESS_CHECK_EVENT_ID => {
            let Some(object_type) = find_prop(&parts.props, "ObjectType") else {
                return (None, None);
            };
            match object_type.trim_matches('"') {
                "File" => (
                    Some(
                        find_prop(&parts.props, "AccessMask")
                            .and_then(|value| parse_u32(value))
                            .map(|mask| access_type_from_mask(mask, false))
                            .unwrap_or(AccessType::Unknown),
                    ),
                    Some(ResourceType::File),
                ),
                "Key" => (
                    Some(
                        find_prop(&parts.props, "AccessMask")
                            .and_then(|value| parse_u32(value))
                            .map(|mask| access_type_from_mask(mask, true))
                            .unwrap_or(AccessType::Unknown),
                    ),
                    Some(ResourceType::Other),
                ),
                "Section" | "SymbolicLink" | "Timer" => (
                    Some(
                        find_prop(&parts.props, "AccessMask")
                            .and_then(|value| parse_u32(value))
                            .map(|mask| {
                                named_object_access_type(object_type.trim_matches('"'), mask)
                            })
                            .unwrap_or(AccessType::Unknown),
                    ),
                    Some(ResourceType::Other),
                ),
                "" => (Some(AccessType::Unknown), Some(ResourceType::Capability)),
                _ => (None, None),
            }
        }
        LEARNING_MODE_VIOLATION_EVENT_ID => (Some(AccessType::Unknown), Some(ResourceType::Ui)),
        CAPABILITY_DENIAL_EVENT_ID if is_ui_violation_schema(parts) => {
            (Some(AccessType::Unknown), Some(ResourceType::Ui))
        }
        CAPABILITY_DENIAL_EVENT_ID => (Some(AccessType::Unknown), Some(ResourceType::Capability)),
        _ => (None, None),
    }
}

/// Whether a provider/event pair belongs to the Learning Mode capture schema.
pub(crate) fn is_learning_mode_event(provider: GUID, event_id: u16) -> bool {
    if provider == KERNEL_GENERAL_PROVIDER {
        matches!(
            event_id,
            ACCESS_CHECK_EVENT_ID | LEARNING_MODE_VIOLATION_EVENT_ID | CAPABILITY_DENIAL_EVENT_ID
        )
    } else if provider == PRIVACY_LEARNING_MODE_PROVIDER {
        matches!(
            event_id,
            ACCESS_CHECK_EVENT_ID
                | LEARNING_MODE_VIOLATION_EVENT_ID
                | PRIVACY_ACCESS_CHECK_EVENT_ID
        )
    } else {
        false
    }
}

pub(crate) fn effective_event_pid(parts: &DecodedEventParts, header_pid: u32) -> Option<u32> {
    if parts.event_id == CAPABILITY_DENIAL_EVENT_ID {
        effective_capability_event_pid(
            find_prop(&parts.props, "ProcessId").map(std::string::String::as_str),
        )
    } else {
        Some(header_pid)
    }
}

fn is_ui_violation_schema(parts: &DecodedEventParts) -> bool {
    parts.event_id == CAPABILITY_DENIAL_EVENT_ID
        && find_prop(&parts.props, "Detail").is_some()
        && find_prop(&parts.props, "Category").is_some()
}

pub(crate) fn effective_capability_event_pid(process_id: Option<&str>) -> Option<u32> {
    process_id.and_then(parse_u32)
}

/// Maps a raw ETW provider GUID to its symbolic verbose logging category.
///
/// Returns `None` for providers outside the Learning Mode vocabulary; those
/// events are ignored entirely (not aggregated), since they are unrelated
/// host traffic rather than an excluded Learning Mode outcome.
pub(crate) fn verbose_logging_provider_for_guid(provider: GUID) -> Option<VerboseLoggingProvider> {
    if provider == KERNEL_GENERAL_PROVIDER {
        Some(VerboseLoggingProvider::KernelGeneral)
    } else if provider == PRIVACY_LEARNING_MODE_PROVIDER {
        Some(VerboseLoggingProvider::PrivacyAuditingPermissiveLearningMode)
    } else {
        None
    }
}

/// Renders a symbolic verbose logging provider category as its raw ETW
/// provider GUID, in stable uppercase braced form, for retention in a verbose
/// logging signature. Provider GUIDs are stable component identifiers, not
/// personal data, so unlike account/user values they are never redacted.
pub(crate) fn verbose_logging_provider_guid(provider: VerboseLoggingProvider) -> String {
    match provider {
        VerboseLoggingProvider::KernelGeneral => {
            format_guid_braced_uppercase(KERNEL_GENERAL_PROVIDER)
        }
        VerboseLoggingProvider::PrivacyAuditingPermissiveLearningMode => {
            format_guid_braced_uppercase(PRIVACY_LEARNING_MODE_PROVIDER)
        }
    }
}

// Keep the serialized spelling independent of formatting changes in the
// `windows` crate: verbose signatures require braces and uppercase hex.
fn format_guid_braced_uppercase(guid: GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7],
    )
}

// ---- verbose logging signature sanitization -------------------------------------
//
// A verbose logging signature retains identifiers useful for triage (SIDs,
// capability names, PIDs, provider/object GUIDs) but must never leak a
// human account/user name, a file path, an exact timestamp, or a free-form
// decoder error message. These bounds apply uniformly to every decoded
// property, in a fixed order: drop timestamp-like properties outright,
// redact sensitive content, *then* bound the surviving property count and
// value length — sanitizing always happens before truncation so a redaction
// is never chopped in half by a length cap.

/// Fixed replacement for any redacted username/account-name value.
pub(crate) const REDACTED_USER: &str = "<redacted-user>";
/// Fixed replacement for a complete file path.
pub(crate) const REDACTED_PATH: &str = "<REDACTED>";
/// Maximum number of `(name, value)` properties retained in one verbose logging
/// signature. Bounds pathological/huge TDH property lists; sanitization
/// (never raw decoding) determines which survive, via deterministic
/// (sorted-by-name) truncation.
pub(crate) const MAX_SIGNATURE_PROPERTIES: usize = 24;
/// Maximum retained length (in `char`s) of one sanitized property value.
pub(crate) const MAX_SIGNATURE_VALUE_LEN: usize = 256;
const SHA256_HEX_LEN: usize = 64;
const BOUNDED_VALUE_MARKER_LEN: usize = "...<sha256=".len() + SHA256_HEX_LEN + ">...".len();
const _: () = assert!(MAX_SIGNATURE_VALUE_LEN > BOUNDED_VALUE_MARKER_LEN);
const TIMESTAMP_PROPERTY_NAMES: &[&str] = &[
    "time",
    "timestamp",
    "filetime",
    "systemtime",
    "eventtime",
    "timecreated",
    "creationtime",
    "createtime",
    "starttime",
    "endtime",
    "exittime",
    "lastwritetime",
];
const RETAINED_IDENTIFIER_SUFFIXES: &[&str] = &["sid", "guid", "identifier", "id"];
const IDENTITY_PROPERTY_NAMES: &[&str] = &["user", "account", "owner", "upn"];
const IDENTITY_PROPERTY_SUFFIXES: &[&str] = &[
    "username",
    "accountname",
    "ownername",
    "principalname",
    "account",
    "upn",
];

#[derive(Clone, Copy)]
struct NormalizedPropertyName<'a>(&'a str);

impl<'a> NormalizedPropertyName<'a> {
    fn bytes(self) -> impl DoubleEndedIterator<Item = u8> + Clone + 'a {
        self.0
            .bytes()
            .filter(|byte| !matches!(byte, b'_' | b'-'))
            .map(|byte| byte.to_ascii_lowercase())
    }

    fn equals(self, expected: &str) -> bool {
        self.bytes().eq(expected.bytes())
    }

    fn ends_with(self, suffix: &str) -> bool {
        let mut bytes = self.bytes().rev();
        suffix
            .bytes()
            .rev()
            .all(|expected| bytes.next() == Some(expected))
    }
}

/// Returns whether `name` looks like it carries a timestamp, so it is
/// dropped from a verbose logging signature entirely (never redacted or
/// truncated) — exact timestamps must not prevent otherwise-identical
/// events from deduplicating. These are trusted ETW schema property names, so
/// matching is deliberately limited to known names and suffixes rather than
/// fuzzy spellings that could discard unrelated properties.
fn is_timestamp_like_property(name: &str) -> bool {
    let normalized = NormalizedPropertyName(name);
    TIMESTAMP_PROPERTY_NAMES
        .iter()
        .copied()
        .any(|expected| normalized.equals(expected))
        || normalized.ends_with("timestamp")
        || normalized.ends_with("filetime")
}

/// Returns whether `name` is itself a standalone user/account-identity
/// property (as opposed to a path that merely *contains* a username), so
/// its value is replaced outright with [`REDACTED_USER`] regardless of
/// content.
fn is_identity_property(name: &str) -> bool {
    let normalized = NormalizedPropertyName(name);
    if RETAINED_IDENTIFIER_SUFFIXES
        .iter()
        .copied()
        .any(|suffix| normalized.equals(suffix) || normalized.ends_with(suffix))
    {
        return false;
    }
    IDENTITY_PROPERTY_NAMES
        .iter()
        .copied()
        .any(|expected| normalized.equals(expected))
        || IDENTITY_PROPERTY_SUFFIXES
            .iter()
            .copied()
            .any(|suffix| normalized.ends_with(suffix))
}

fn looks_like_file_path_property(name: &str, value: &str, object_type: Option<&str>) -> bool {
    let normalized = NormalizedPropertyName(name);
    if normalized.ends_with("path") || normalized.ends_with("filename") {
        return true;
    }

    if !normalized.equals("objectname") && !normalized.equals("resource") {
        return false;
    }
    if object_type.is_some_and(|object_type| object_type.eq_ignore_ascii_case("File")) {
        return true;
    }

    crate::path_norm::is_user_visible_absolute(value)
        || looks_like_dos_device_filesystem_path(value)
        || looks_like_nt_filesystem_path(value)
}

fn looks_like_dos_device_filesystem_path(value: &str) -> bool {
    let rest = [r"\??\", r"\\?\", r"\\.\"]
        .into_iter()
        .find_map(|prefix| strip_prefix_ignore_ascii_case(value, prefix));
    let Some(rest) = rest else {
        return false;
    };
    if looks_like_drive_absolute_path(rest)
        || strip_prefix_ignore_ascii_case(rest, r"UNC\").is_some()
    {
        return true;
    }

    let Some(volume) = strip_prefix_ignore_ascii_case(rest, "Volume{") else {
        return false;
    };
    volume.contains(r"}\")
}

fn looks_like_drive_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn looks_like_nt_filesystem_path(value: &str) -> bool {
    let Some(rest) = strip_prefix_ignore_ascii_case(value, r"\Device\") else {
        return false;
    };
    let Some((device, _path)) = rest.split_once('\\') else {
        return false;
    };
    if device.eq_ignore_ascii_case("Mup") || ends_with_ignore_ascii_case(device, "Redirector") {
        return true;
    }

    let Some(volume_number) = strip_prefix_ignore_ascii_case(device, "HarddiskVolume") else {
        return false;
    };
    !volume_number.is_empty() && volume_number.bytes().all(|byte| byte.is_ascii_digit())
}

fn strip_prefix_ignore_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .then(|| &value[prefix.len()..])
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(suffix))
}

/// Deterministically discards sorted properties after
/// [`MAX_SIGNATURE_PROPERTIES`] and bounds retained values to
/// [`MAX_SIGNATURE_VALUE_LEN`] characters.
///
/// Overlong values retain prefix and suffix context plus a digest of the full
/// sanitized value. The digest prevents distinct values with a long shared
/// prefix from collapsing into one verbose logging signature.
///
/// Callers must sanitize (redact) *before* calling this so neither the retained
/// context nor the digest is derived from sensitive identity content.
pub(crate) fn bound_properties(mut properties: Vec<(String, String)>) -> Vec<(String, String)> {
    properties.truncate(MAX_SIGNATURE_PROPERTIES);
    for (_, value) in &mut properties {
        let char_count = value.chars().count();
        if char_count > MAX_SIGNATURE_VALUE_LEN {
            *value = bound_property_value(value, char_count);
        }
    }
    properties
}

fn bound_property_value(value: &str, char_count: usize) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let marker = format!("...<sha256={digest:x}>...");
    debug_assert_eq!(marker.len(), BOUNDED_VALUE_MARKER_LEN);
    let context_len = MAX_SIGNATURE_VALUE_LEN - BOUNDED_VALUE_MARKER_LEN;
    let prefix_len = context_len.div_ceil(2);
    let suffix_len = context_len / 2;
    let prefix = value.chars().take(prefix_len).collect::<String>();
    let suffix = value
        .chars()
        .skip(char_count - suffix_len)
        .collect::<String>();
    format!("{prefix}{marker}{suffix}")
}

/// Produces the deterministic, sanitized, bounded property list for a verbose
/// logging signature from one decoded event's raw TDH properties.
///
/// Never includes a free-form decoder error message (decode failures are
/// recorded with an empty property list by the caller, before this
/// function would ever run) and never includes a timestamp-like property,
/// so exact-timestamp differences between otherwise-identical events don't
/// prevent their signatures from deduplicating. SIDs, capability names,
/// PIDs carried as properties, and GUID-shaped values are retained
/// verbatim; account/user-identity content and complete file paths are
/// redacted. Classification is schema-independent: newly decoded properties
/// automatically pass through these name/value checks without requiring a new
/// extractor branch.
pub(crate) fn sanitize_properties(props: &[(String, String)]) -> Vec<(String, String)> {
    let usernames = props
        .iter()
        .filter(|(name, _)| is_identity_property(name))
        .flat_map(|(_, value)| username_match_candidates(value.trim_matches('"')))
        .collect::<Vec<_>>();
    let object_type = props
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("ObjectType"))
        .map(|(_, value)| value.trim_matches('"'));
    let mut sanitized = std::collections::BTreeMap::new();
    for (name, raw_value) in props {
        if is_timestamp_like_property(name) {
            continue;
        }
        let value = raw_value.trim_matches('"');
        let sanitized_value = if is_identity_property(name) {
            REDACTED_USER.to_string()
        } else if looks_like_file_path_property(name, value, object_type) {
            REDACTED_PATH.to_string()
        } else {
            redact_known_username_components(value, &usernames)
        };
        sanitized.insert(name.clone(), sanitized_value);
    }

    fn username_match_candidates(identity: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        let account = identity
            .rsplit_once('\\')
            .map_or(identity, |(_, account)| account);
        for candidate in [
            identity,
            account,
            account.split_once('@').map_or(account, |(name, _)| name),
        ] {
            if !candidate.is_empty()
                && !candidates
                    .iter()
                    .any(|existing: &String| windows_paths_equal_ignore_case(existing, candidate))
            {
                candidates.push(candidate.to_string());
            }
        }
        candidates
    }

    fn redact_known_username_components(value: &str, usernames: &[String]) -> String {
        value
            .split_inclusive(['\\', '/'])
            .map(|component| {
                let (segment, separator) = if component.ends_with('\\') || component.ends_with('/')
                {
                    component.split_at(component.len() - 1)
                } else {
                    (component, "")
                };
                if usernames
                    .iter()
                    .any(|username| windows_paths_equal_ignore_case(segment, username))
                {
                    format!("{REDACTED_USER}{separator}")
                } else {
                    component.to_string()
                }
            })
            .collect()
    }
    bound_properties(sanitized.into_iter().collect())
}

///
/// The `ObjectType` field selects the resource type: `File` and `Key`
/// (registry) map to concrete resources, an **empty** `ObjectType` is a
/// brokered-capability check, and the observed named-object types `Section`,
/// `SymbolicLink`, and `Timer` map to [`ResourceType::Other`]. Only registry
/// reads are actionable; other registry access and those named-object types are
/// excluded because MXC has no corresponding policy grants. Other object types
/// are dropped until their access-mask vocabulary is understood. An absent
/// `ObjectType` field drops the event.
///
/// For file/registry resources the [`AccessType`] is derived from the
/// event's `AccessMask` field (the desired access the caller was denied;
/// see [`access_type_from_mask`]); when the field is absent or unparseable
/// the type falls back to [`AccessType::Unknown`] so a decode gap never
/// drops the denial itself. Capability checks carry a mask that is not a
/// read/write/execute verb, so their access type is left `Unknown`.
///
/// # Errors
///
/// Returns the closed [`VerboseLoggingOutcomeReason`] describing why no denial
/// could be built: a missing `ObjectType` ([`VerboseLoggingOutcomeReason::MissingObjectType`]),
/// an object type this model can't represent
/// ([`VerboseLoggingOutcomeReason::UnsupportedObjectType`]), a missing/empty
/// object name (registry/file:
/// [`VerboseLoggingOutcomeReason::MissingObjectName`]; capability: an
/// unidentified brokered check,
/// [`VerboseLoggingOutcomeReason::UnresolvedCapability`] — [`crate::capability_dacl`]
/// may still recover it from the event's DACL payload), or a self-access,
/// non-read registry, or recognized named-object check that isn't actionable
/// ([`VerboseLoggingOutcomeReason::NotActionable`]).
pub fn build_denial_from_access_check(
    parts: &DecodedEventParts,
    pid: u32,
    filetime: u64,
    provider: VerboseLoggingProvider,
) -> Result<RawDenial, VerboseLoggingOutcomeReason> {
    let object_type = find_prop(&parts.props, "ObjectType")
        .ok_or(VerboseLoggingOutcomeReason::MissingObjectType)?;
    let object_type_str = object_type.trim_matches('"');

    let resource_type = match object_type_str {
        "File" => ResourceType::File,
        "Key" => ResourceType::Other,
        "Section" | "SymbolicLink" | "Timer" => ResourceType::Other,
        // A present-but-empty object type is a brokered-capability check.
        "" => ResourceType::Capability,
        _ => return Err(VerboseLoggingOutcomeReason::UnsupportedObjectType),
    };

    let object_name = find_prop(&parts.props, "ObjectName")
        .map(|v| v.trim_matches('"').to_string())
        .filter(|name| !name.is_empty());
    let object_name = match (resource_type, object_name) {
        // An unidentified brokered-capability check: the identifier may
        // still be recoverable from the event's DACL payload.
        (ResourceType::Capability, None) => {
            return Err(VerboseLoggingOutcomeReason::UnresolvedCapability)
        }
        (_, None) => return Err(VerboseLoggingOutcomeReason::MissingObjectName),
        (_, Some(name)) => name,
    };

    if resource_type == ResourceType::File {
        let app_path = find_prop(&parts.props, "AppPath")
            .or_else(|| find_prop(&parts.props, "ApplicationPath"))
            .map(|value| value.trim_matches('"'));
        if app_path.is_some_and(|app_path| is_self_access(&object_name, app_path)) {
            return Err(VerboseLoggingOutcomeReason::NotActionable);
        }
    }

    let access_type = if resource_type == ResourceType::Capability {
        // Capability checks report a mask (often 0x1) that is not a
        // read/write/execute verb, so don't run the file/registry
        // classifier over it.
        AccessType::Unknown
    } else {
        find_prop(&parts.props, "AccessMask")
            .and_then(|v| parse_u32(v))
            .map(|mask| match object_type_str {
                "Key" => access_type_from_mask(mask, true),
                "File" => access_type_from_mask(mask, false),
                _ => named_object_access_type(object_type_str, mask),
            })
            .unwrap_or(AccessType::Unknown)
    };

    if (object_type_str == "Key" && access_type != AccessType::Read)
        || matches!(object_type_str, "Section" | "SymbolicLink" | "Timer")
    {
        return Err(VerboseLoggingOutcomeReason::NotActionable);
    }

    Ok(RawDenial {
        pid,
        resource_type,
        object_name,
        access_type,
        filetime,
        event_id: parts.event_id,
        provider,
        verbose_logging_properties: sanitize_properties(&parts.props),
    })
}

fn is_self_access(object_name: &str, app_path: &str) -> bool {
    let object_name = strip_dos_namespace_prefix(object_name);
    let app_path = strip_dos_namespace_prefix(app_path);
    match (path_namespace(object_name), path_namespace(app_path)) {
        (Some(object_namespace), Some(app_namespace)) if object_namespace == app_namespace => {
            windows_paths_equal_ignore_case(object_name, app_path)
        }
        (Some(PathNamespace::Dos), Some(PathNamespace::DeviceVolume)) => {
            crate::path_norm::device_path_matches_dos(app_path, object_name)
        }
        (Some(PathNamespace::DeviceVolume), Some(PathNamespace::Dos)) => {
            crate::path_norm::device_path_matches_dos(object_name, app_path)
        }
        _ => false,
    }
}

fn windows_paths_equal_ignore_case(a: &str, b: &str) -> bool {
    wxc_common::string_util::windows_paths_equal_ignore_case(a, b)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PathNamespace {
    Dos,
    DeviceVolume,
}

fn path_namespace(path: &str) -> Option<PathNamespace> {
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
        return Some(PathNamespace::Dos);
    }

    const VOLUME_PREFIX: &str = r"\Device\HarddiskVolume";
    path.get(..VOLUME_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(VOLUME_PREFIX))
        .then_some(PathNamespace::DeviceVolume)
}

fn strip_dos_namespace_prefix(path: &str) -> &str {
    for prefix in [r"\??\", r"\\?\", r"\\.\"] {
        if let Some(path) = path.strip_prefix(prefix) {
            return path;
        }
    }
    path
}

/// Builds a [`RawDenial`] from a `LearningModeViolation` (event 27) payload.
///
/// These represent UI-surface denials. `Category` identifies the class and
/// `Detail` identifies the concrete UI operation; `ProcessName` is the caller
/// and must not be emitted as the denied resource.
///
/// # Errors
///
/// Returns [`VerboseLoggingOutcomeReason::MissingObjectType`] when `Category` is
/// absent or unparseable, [`VerboseLoggingOutcomeReason::MissingObjectName`]
/// when the required `Detail` is absent or unparseable, and
/// [`VerboseLoggingOutcomeReason::NotActionable`] when the category/detail pair
/// describes no violation (`Category == 0`).
pub fn build_denial_from_learning_mode(
    parts: &DecodedEventParts,
    pid: u32,
    filetime: u64,
    provider: VerboseLoggingProvider,
) -> Result<RawDenial, VerboseLoggingOutcomeReason> {
    let category = find_prop(&parts.props, "Category")
        .and_then(|value| parse_u32(value))
        .ok_or(VerboseLoggingOutcomeReason::MissingObjectType)?;
    let detail = match find_prop(&parts.props, "Detail") {
        Some(value) => parse_u32(value).ok_or(VerboseLoggingOutcomeReason::MissingObjectName)?,
        None if category == crate::ui::CONVERT_TO_GUI => 0,
        None => return Err(VerboseLoggingOutcomeReason::MissingObjectName),
    };
    let object_name = crate::ui::resource_name(category, detail)
        .ok_or(VerboseLoggingOutcomeReason::NotActionable)?;

    Ok(RawDenial {
        pid,
        resource_type: ResourceType::Ui,
        object_name,
        access_type: AccessType::Unknown,
        filetime,
        event_id: parts.event_id,
        provider,
        verbose_logging_properties: sanitize_properties(&parts.props),
    })
}

/// Builds a [`RawDenial`] from a capability-denial (event 28) payload.
///
/// Emitted under `block`. The record reports a `Denied` boolean; we
/// only surface actual denials. The originating process is taken from the
/// payload `ProcessId` (which is more precise than the ETW header pid for
/// brokered checks) when present, else the header pid. The capability name
/// comes from the `PackageSid` capability SID, resolved to its friendly
/// policy name via [`crate::capability_names`] (custom hashed capabilities
/// fall back to the SID string).
///
/// # Errors
///
/// Returns [`VerboseLoggingOutcomeReason::NotActionable`] when `Denied` is
/// absent or not `true`, and [`VerboseLoggingOutcomeReason::UnresolvedCapability`]
/// when no usable capability identifier could be decoded.
pub fn build_denial_from_capability(
    parts: &DecodedEventParts,
    pid: u32,
    filetime: u64,
    provider: VerboseLoggingProvider,
) -> Result<RawDenial, VerboseLoggingOutcomeReason> {
    // A partially decoded event must not become a policy recommendation.
    let denied = find_prop(&parts.props, "Denied")
        .is_some_and(|value| value.trim_matches('"').eq_ignore_ascii_case("true"));
    if !denied {
        return Err(VerboseLoggingOutcomeReason::NotActionable);
    }

    let pid = find_prop(&parts.props, "ProcessId")
        .and_then(|v| parse_u32(v))
        .unwrap_or(pid);

    // Resolve the denied capability's name. The event carries it as a
    // capability SID (`PackageSid`, rendered `S-1-15-3-…` by the TDH SID
    // decoder); map well-known capability SIDs to their friendly policy name
    // and fall back to the SID string for custom (hashed) capabilities.
    let object_name = find_prop(&parts.props, "CapabilityName")
        .or_else(|| find_prop(&parts.props, "Capability"))
        .map(|v| v.trim_matches('"').to_string())
        .or_else(|| {
            find_prop(&parts.props, "PackageSid")
                .or_else(|| find_prop(&parts.props, "CapabilitySid"))
                .or_else(|| find_prop(&parts.props, "Sid"))
                .map(|v| crate::capability_names::resolve(v.trim_matches('"')))
        })
        .filter(|name| {
            !name.is_empty()
                && name != "<unsupported>"
                && name != "<invalid SID>"
                && name != "<malformed-sid>"
        })
        .ok_or(VerboseLoggingOutcomeReason::UnresolvedCapability)?;

    Ok(RawDenial {
        pid,
        resource_type: ResourceType::Capability,
        object_name,
        access_type: AccessType::Unknown,
        filetime,
        event_id: parts.event_id,
        provider,
        verbose_logging_properties: sanitize_properties(&parts.props),
    })
}

/// Parses a `"0x…"` / decimal / bare-hex property value into a `u32`.
///
/// The `AccessMask` and `ProcessId` templates render as `win:HexInt32`, so
/// the TDH decoder emits a `"0x…"` string (e.g. `"0x120089"`, `"0x1acc"`).
/// We accept a leading `0x`/`0X` (hex) and, defensively, a bare decimal or
/// bare-hex form in case a future decoder path formats it differently.
/// Returns `None` when the value can't be parsed as a 32-bit integer.
fn parse_u32(raw: &str) -> Option<u32> {
    let s = raw.trim().trim_matches('"').trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok();
    }
    // No explicit prefix: prefer decimal, fall back to hex.
    s.parse::<u32>()
        .ok()
        .or_else(|| u32::from_str_radix(s, 16).ok())
}

/// Classifies a Windows access mask into a single [`AccessType`].
///
/// A mask often requests several rights at once (e.g. `FILE_GENERIC_READ`
/// bundles multiple read bits). Since `AccessType` is single-valued, we
/// return the highest-privilege intent present, in the order
/// **Write → Execute → Read**, so an approval that grants the reported type
/// also covers everything else the caller asked for. Mutating rights that
/// have no dedicated variant (delete, create, take-ownership, change DACL)
/// fold into `Write`. A mask with no recognised right (e.g. only
/// `SYNCHRONIZE` or `MAXIMUM_ALLOWED`) yields [`AccessType::Unknown`].
///
/// `is_registry` selects the object-specific low-bit vocabulary: files and
/// registry keys share the standard/generic bits but disagree on bits like
/// `0x10` (`FILE_WRITE_EA` vs `KEY_NOTIFY`) and `0x20` (`FILE_EXECUTE` vs
/// `KEY_CREATE_LINK`).
fn access_type_from_mask(mask: u32, is_registry: bool) -> AccessType {
    // Standard rights (object-type independent).
    const DELETE: u32 = 0x0001_0000;
    const WRITE_DAC: u32 = 0x0004_0000;
    const WRITE_OWNER: u32 = 0x0008_0000;
    // Generic rights (object-type independent).
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const GENERIC_EXECUTE: u32 = 0x2000_0000;
    const GENERIC_ALL: u32 = 0x1000_0000;

    let standard_write = DELETE | WRITE_DAC | WRITE_OWNER;

    let (read_bits, write_bits, execute_bits) = if is_registry {
        // Registry key-specific rights (winnt.h KEY_*).
        const KEY_QUERY_VALUE: u32 = 0x0001;
        const KEY_SET_VALUE: u32 = 0x0002;
        const KEY_CREATE_SUB_KEY: u32 = 0x0004;
        const KEY_ENUMERATE_SUB_KEYS: u32 = 0x0008;
        const KEY_NOTIFY: u32 = 0x0010;
        const KEY_CREATE_LINK: u32 = 0x0020;
        (
            KEY_QUERY_VALUE | KEY_ENUMERATE_SUB_KEYS | KEY_NOTIFY | GENERIC_READ | GENERIC_EXECUTE,
            KEY_SET_VALUE
                | KEY_CREATE_SUB_KEY
                | KEY_CREATE_LINK
                | standard_write
                | GENERIC_WRITE
                | GENERIC_ALL,
            // Registry has no execute concept (KEY_EXECUTE aliases KEY_READ).
            0,
        )
    } else {
        // File/directory-specific rights (winnt.h FILE_*).
        const FILE_READ_DATA: u32 = 0x0001; // a.k.a. FILE_LIST_DIRECTORY
        const FILE_WRITE_DATA: u32 = 0x0002; // a.k.a. FILE_ADD_FILE
        const FILE_APPEND_DATA: u32 = 0x0004; // a.k.a. FILE_ADD_SUBDIRECTORY
        const FILE_READ_EA: u32 = 0x0008;
        const FILE_WRITE_EA: u32 = 0x0010;
        const FILE_EXECUTE: u32 = 0x0020; // a.k.a. FILE_TRAVERSE
        const FILE_DELETE_CHILD: u32 = 0x0040;
        const FILE_READ_ATTRIBUTES: u32 = 0x0080;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        (
            FILE_READ_DATA | FILE_READ_EA | FILE_READ_ATTRIBUTES | GENERIC_READ,
            FILE_WRITE_DATA
                | FILE_APPEND_DATA
                | FILE_WRITE_EA
                | FILE_DELETE_CHILD
                | FILE_WRITE_ATTRIBUTES
                | standard_write
                | GENERIC_WRITE
                | GENERIC_ALL,
            FILE_EXECUTE | GENERIC_EXECUTE,
        )
    };

    if mask & write_bits != 0 {
        AccessType::Write
    } else if mask & execute_bits != 0 {
        AccessType::Execute
    } else if mask & read_bits != 0 {
        AccessType::Read
    } else {
        AccessType::Unknown
    }
}

fn named_object_access_type(object_type: &str, mask: u32) -> AccessType {
    let (read_bits, write_bits, execute_bits) = match object_type {
        // ntifs.h SECTION_* rights.
        "Section" => (0x0001 | 0x0004, 0x0002 | 0x0010, 0x0008 | 0x0020),
        // ntifs.h SYMBOLIC_LINK_* rights.
        "SymbolicLink" => (0x0001, 0x0002, 0),
        // winnt.h TIMER_* rights.
        "Timer" => (0x0001, 0x0002, 0),
        _ => return AccessType::Unknown,
    };

    // Standard and generic rights are object-type independent.
    const DELETE: u32 = 0x0001_0000;
    const WRITE_DAC: u32 = 0x0004_0000;
    const WRITE_OWNER: u32 = 0x0008_0000;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const GENERIC_EXECUTE: u32 = 0x2000_0000;
    const GENERIC_ALL: u32 = 0x1000_0000;

    if mask & (write_bits | DELETE | WRITE_DAC | WRITE_OWNER | GENERIC_WRITE | GENERIC_ALL) != 0 {
        AccessType::Write
    } else if mask & (execute_bits | GENERIC_EXECUTE) != 0 {
        AccessType::Execute
    } else if mask & (read_bits | GENERIC_READ) != 0 {
        AccessType::Read
    } else {
        AccessType::Unknown
    }
}

fn find_prop<'a>(props: &'a [(String, String)], name: &str) -> Option<&'a String> {
    props.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}
