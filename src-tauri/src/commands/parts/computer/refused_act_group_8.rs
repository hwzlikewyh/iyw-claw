// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// An action the helper refused, or that did not happen, in the helper's
/// words (which say what to do next).
pub(super) fn refused_act(kind: ActRefusal, words: String) -> Refusal {
    match kind {
        ActRefusal::Paused => Refusal::refused(
            ERROR_PAUSED,
            format!("{words} Nothing reaches any window until then; try again later."),
        ),
        ActRefusal::Stopped => stopped(),
        ActRefusal::StaleRef => Refusal::failed(ERROR_STALE_REF, words),
        ActRefusal::OutOfTarget => Refusal::failed(ERROR_OUT_OF_TARGET, words),
        ActRefusal::Occluded => Refusal::failed(ERROR_OCCLUDED, words),
        ActRefusal::BackgroundUnavailable => Refusal::failed(ERROR_BACKGROUND_UNAVAILABLE, words),
        ActRefusal::SecretField => Refusal::refused(ERROR_BLOCKED, words),
        ActRefusal::Failed => Refusal::failed(ERROR_ACTION_FAILED, words),
        ActRefusal::Paste => Refusal::refused(ERROR_GRANT_REQUIRED, words),
        ActRefusal::Beyond => Refusal::refused(ERROR_CONTROL_REQUIRED, words),
        ActRefusal::Revoked => Refusal::refused(ERROR_GRANT_REQUIRED, words),
    }
}

/// A call the person's Stop cut off.
pub(super) fn stopped() -> Refusal {
    Refusal::refused(ERROR_STOPPED, STOPPED_NOTE.to_string())
}

/// How an action is to reach its window: as the agent asked, or as the
/// person set it when it did not ask — the front only where they allow it,
/// and only for an action that can be delivered there at all (see
/// [`ComputerActRequest::can_come_forward`]).
pub(super) fn delivery_for(
    request: &ComputerActRequest,
    requested: Option<ActDelivery>,
    config: &ComputerToolsConfig,
) -> Result<ActDelivery, Refusal> {
    delivery_on(
        request,
        requested,
        config,
        crate::computer::keys::Platform::current(),
    )
}

/// [`delivery_for`] on `platform`: an action that can be done there only at
/// the front (see [`ComputerActRequest::needs_front`]) goes there where the
/// person allows it, and not at all where they do not.
pub(super) fn delivery_on(
    request: &ComputerActRequest,
    requested: Option<ActDelivery>,
    config: &ComputerToolsConfig,
    platform: crate::computer::keys::Platform,
) -> Result<ActDelivery, Refusal> {
    if request.needs_front(platform) {
        let note = match request {
            ComputerActRequest::InvokeMenu { .. } => MENU_NEEDS_FRONT_NOTE,
            _ => RESTORE_NEEDS_FRONT_NOTE,
        };
        return if config.allow_foreground {
            Ok(ActDelivery::Foreground)
        } else {
            Err(Refusal::refused(
                ERROR_FOREGROUND_NOT_ALLOWED,
                note.to_string(),
            ))
        };
    }
    if !request.can_come_forward() {
        return Ok(ActDelivery::Background);
    }
    match requested.unwrap_or_else(|| config.default_delivery_in_force()) {
        ActDelivery::Foreground if !config.allow_foreground => Err(Refusal::refused(
            ERROR_FOREGROUND_NOT_ALLOWED,
            FOREGROUND_NOT_ALLOWED_NOTE.to_string(),
        )),
        delivery => Ok(delivery),
    }
}

/// An action the application would not take in the background, ended with
/// what the person's settings leave the agent to try next: the front, or
/// asking them for it — for an action that can come to the front at all.
/// The helper's words say what happened; only iyw-claw knows the settings.
pub(super) fn with_next_step(
    refusal: Refusal,
    request: &ComputerActRequest,
    config: &ComputerToolsConfig,
) -> Refusal {
    if refusal.slug != ERROR_BACKGROUND_UNAVAILABLE || !request.can_come_forward() {
        return refusal;
    }
    Refusal {
        note: format!(
            "{} {}",
            refusal.note,
            background_next_step(config.allow_foreground)
        ),
        ..refusal
    }
}

/// An action refused before anything was sent, in words.
pub(super) fn denied(target_id: &str, why: ActDenied) -> Refusal {
    match why {
        ActDenied::NoSuchTarget => {
            Refusal::refused(ERROR_NO_SUCH_TARGET, no_such_target_note(target_id))
        }
        ActDenied::GrantRequired => {
            Refusal::refused(ERROR_GRANT_REQUIRED, grant_required_note(target_id))
        }
        ActDenied::ControlRequired => {
            Refusal::refused(ERROR_CONTROL_REQUIRED, control_required_note(target_id))
        }
        ActDenied::NotGrantable(why) => {
            Refusal::refused(ERROR_BLOCKED, blocked_note(target_id, why.note()))
        }
        ActDenied::Stale(staleness) => Refusal::failed(
            ERROR_STALE_REF,
            match staleness {
                Staleness::NoSnapshot | Staleness::OldSnapshot => stale_snapshot_note(target_id),
                Staleness::NotActionable => not_actionable_note(target_id),
                Staleness::CutAway(index) => cut_away_note(index),
                Staleness::NoSuchRef(index) => no_such_ref_note(target_id, index),
                Staleness::NoCapture | Staleness::OldCapture => stale_capture_note(target_id),
            },
        ),
        ActDenied::OutOfImage => Refusal::failed(ERROR_OUT_OF_TARGET, OUT_OF_IMAGE_NOTE.into()),
        ActDenied::Secret => Refusal::refused(ERROR_BLOCKED, SECRET_FIELD_NOTE.into()),
        ActDenied::ChordBeyond => Refusal::refused(ERROR_CONTROL_REQUIRED, chord_beyond_note()),
        // The source of what is on the clipboard is what a paste would need
        // a grant for; iyw-claw does not know it.
        ActDenied::Paste => Refusal::refused(ERROR_GRANT_REQUIRED, PASTE_NOTE.into()),
        ActDenied::NeedsElement => Refusal::failed(ERROR_ACTION_FAILED, NEEDS_ELEMENT_NOTE.into()),
        ActDenied::NoPointing => Refusal::failed(ERROR_ACTION_FAILED, no_pointing_note(target_id)),
        ActDenied::DragModifiers => {
            Refusal::failed(ERROR_ACTION_FAILED, DRAG_MODIFIERS_NOTE.into())
        }
        ActDenied::DoubleClickModifiers => {
            Refusal::failed(ERROR_ACTION_FAILED, DOUBLE_CLICK_MODIFIERS_NOTE.into())
        }
        ActDenied::DesktopChord => {
            Refusal::refused(ERROR_CONTROL_REQUIRED, DESKTOP_CHORD_NOTE.into())
        }
        ActDenied::AppGrantRequired => {
            Refusal::refused(ERROR_CONTROL_REQUIRED, app_grant_required_note(target_id))
        }
        ActDenied::MenusUnavailable => {
            Refusal::failed(ERROR_ACTION_FAILED, MENUS_UNAVAILABLE_NOTE.into())
        }
        ActDenied::BadFrame => Refusal::failed(ERROR_ACTION_FAILED, BAD_FRAME_NOTE.into()),
        ActDenied::SessionChord => {
            Refusal::refused(ERROR_CONTROL_REQUIRED, SESSION_CHORD_NOTE.into())
        }
        ActDenied::ScreenPointerOnly => {
            Refusal::failed(ERROR_ACTION_FAILED, SCREEN_POINTER_ONLY_NOTE.into())
        }
    }
}

/// An action on the entire screen refused before anything was sent, in
/// words about the screen.
pub(super) fn screen_denied(why: ActDenied) -> Refusal {
    match why {
        ActDenied::GrantRequired | ActDenied::NoSuchTarget => {
            Refusal::refused(ERROR_GRANT_REQUIRED, SCREEN_GRANT_REQUIRED_NOTE.into())
        }
        ActDenied::ControlRequired => {
            Refusal::refused(ERROR_CONTROL_REQUIRED, SCREEN_CONTROL_REQUIRED_NOTE.into())
        }
        ActDenied::Stale(_) => Refusal::failed(ERROR_STALE_REF, SCREEN_STALE_CAPTURE_NOTE.into()),
        other => denied(SCREEN_TARGET_ID, other),
    }
}

pub(super) fn permission_name(permission: OsPermission) -> &'static str {
    match permission {
        OsPermission::Accessibility => "Accessibility",
        OsPermission::ScreenRecording => "Screen Recording",
    }
}

/// A window or an application asked to change while the entire screen is
/// shared.
pub(super) fn screen_shared_error() -> AppCommandError {
    AppCommandError::configuration_invalid(
        "the entire screen is shared, which every window is shared with; change the screen's \
         sharing instead",
    )
}

/// The blocklist the settings `config` make: the defaults less those taken
/// off, plus the user's own.
pub(super) fn blocklist_of(config: &ComputerToolsConfig) -> Blocklist {
    Blocklist::configured(&config.blocklist, &config.blocklist_removed)
}

/// What a share is decided by: see `ComputerService::policy`.
pub(super) struct SharingPolicy {
    pub(in crate::commands::computer) enabled: bool,
    /// The entire screen may be shared.
    pub(in crate::commands::computer) screen_enabled: bool,
    pub(in crate::commands::computer) blocklist: Blocklist,
}
