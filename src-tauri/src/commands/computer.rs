//! Computer Use 宿主服务：持有 helper、目标表、Agent 工具和共享面板命令。
//! 读取按顺序检查：实时开关、共享权限/期限/黑名单、内核进程身份、
//! helper 读取、再次检查所有条件及停止代次，最后记录操作结果。
//! 读取返回前撤销的授权不会泄露其内容；进程重启也不会继承旧授权。
//!
//! 动作要求控制权限，并按最近截图/快照解析坐标和元素引用。
//! 输入一经发出无法撤回，因此宿主在出队时检查，helper 在实际发送前再检查。
//! 驱动调用串行；停止走独立通道，先递增代次并撤销共享，再中断驱动。
//! 旧排队动作不能在重新共享后执行，结果不明的输入不能自动重放。
//!
//! 桌面模式通过本机 IPC 共享窗口；显式启用桌面功能的服务端通过已鉴权的
//! HTTP 面板共享远端宿主。Agent 工具不能创建共享授权。

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
#[cfg(feature = "tauri-runtime")]
use tauri::{AppHandle, Manager};

use crate::acp::computer_tools::{
    app_grant_required_note, background_next_step, blocked_note, chord_beyond_note,
    control_required_note, cut_away_note, grant_required_note, no_pointing_note, no_such_ref_note,
    no_such_target_note, not_actionable_note, permission_missing_note, reshared_note,
    stale_capture_note, stale_snapshot_note, ClipboardOp, ComputerActOutcome, ComputerAppsOutcome,
    ComputerCaptureOutcome, ComputerClipboardOutcome, ComputerLaunchOutcome,
    ComputerSnapshotOutcome, ComputerToolAccess, ComputerToolsConfig, ComputerToolsRuntimeConfig,
    ComputerVerifyOutcome, ComputerWindowsOutcome, InputPolicy, SnapshotRequest, BAD_FRAME_NOTE,
    CLIPBOARD_NOT_YOURS_NOTE, CLIPBOARD_OFF_NOTE, CLIPBOARD_WRITTEN_NOTE, DEFAULT_MAX_DIMENSION,
    DEFAULT_SNAPSHOT_MAX_CHARS, DESKTOP_CHORD_NOTE, DOUBLE_CLICK_MODIFIERS_NOTE,
    DRAG_MODIFIERS_NOTE, ERROR_ACTION_FAILED, ERROR_BACKGROUND_UNAVAILABLE, ERROR_BLOCKED,
    ERROR_CONTROL_REQUIRED, ERROR_FOREGROUND_NOT_ALLOWED, ERROR_GRANT_REQUIRED,
    ERROR_NO_SUCH_TARGET, ERROR_OCCLUDED, ERROR_OUT_OF_TARGET, ERROR_PAUSED,
    ERROR_PERMISSION_MISSING, ERROR_READ_FAILED, ERROR_STALE_REF, ERROR_STOPPED, ERROR_UNAVAILABLE,
    FOREGROUND_NOT_ALLOWED_NOTE, LAUNCHED_NOTE, LAUNCH_OFF_NOTE, MENUS_UNAVAILABLE_NOTE,
    MENU_NEEDS_FRONT_NOTE, NEEDS_ELEMENT_NOTE, NO_DESKTOP_NOTE, OUT_OF_IMAGE_NOTE, PASTE_NOTE,
    RESTORE_NEEDS_FRONT_NOTE, SCREEN_CONTROL_REQUIRED_NOTE, SCREEN_GRANT_REQUIRED_NOTE,
    SCREEN_NEEDS_FRONT_NOTE, SCREEN_NOT_BACKGROUND_NOTE, SCREEN_POINTER_ONLY_NOTE,
    SCREEN_RULES_CHANGED_NOTE, SCREEN_STALE_CAPTURE_NOTE, SECRET_FIELD_NOTE, SESSION_CHORD_NOTE,
    STOPPED_NOTE,
};
use crate::app_error::AppCommandError;
use crate::computer::agent::{
    grantable, visible_title, ActivityOutcome, Blocklist, ComputerAction, ComputerActivityPayload,
    ComputerGrantPayload, GrantChange, GrantLevel, GrantScope, NotGrantable, SelfIdentity,
};
use crate::computer::backend::{
    ActRefusal, BackendError, BackendStatus, ComputerBackend, SnapshotOptions,
};
use crate::computer::driver_admin::{DriverAdmin, DriverInfo, DriverTask};
use crate::computer::events::ComputerEvents;
#[cfg(feature = "tauri-runtime")]
use crate::computer::indicator::{Indicator, Strip};
use crate::computer::local::LocalBackend;
#[cfg(feature = "tauri-runtime")]
use crate::computer::marker::Marker;
use crate::computer::procinfo::process_start;
use crate::computer::protocol::{
    ClipboardUse, OsPermission, PermissionAsked, PermissionReport, RawAct, ScreenRules,
};
#[cfg(feature = "tauri-runtime")]
use crate::computer::stop_key::StopKey;
use crate::computer::stop_shortcut::StopKeyStatus;
use crate::computer::targets::{
    ActDenied, Aim, AppChange, AppTarget, ReadMark, ReadRefusal, ReadTicket, ShareError, SharedApp,
    SharedScreen, SharedWindow, Staleness, TargetTable, WindowIdentity, SCREEN_TARGET_ID,
};
use crate::computer::types::{
    ActDelivery, ActReport, AgentAppRef, AgentAppSummary, AgentScreen, ComputerActRequest, Rect,
    VerifyOutcome, VerifyRequest, WindowCapture, WindowSnapshot, MAX_HOLD_MS, MAX_KEY_REPEAT,
};
use crate::web::event_bridge::{EventEmitter, WebEventBroadcaster};
#[path = "parts/computer/EXPIRY_SWEEP_group_1.rs"]
mod part_1;
use part_1::*;
#[path = "parts/computer/HostTccStatus_group_2.rs"]
mod part_2;
pub use part_2::*;
#[path = "parts/computer/is_leaking_group_3.rs"]
mod part_3;
#[cfg(target_os = "macos")]
#[path = "parts/computer/host_tcc_group_4.rs"]
mod part_4;
#[cfg(target_os = "macos")]
use part_4::*;
#[cfg(not(target_os = "macos"))]
#[path = "parts/computer/host_tcc_group_5.rs"]
mod part_5;
#[cfg(not(target_os = "macos"))]
use part_5::*;
#[path = "parts/computer/cut_tree_group_6.rs"]
mod part_6;
use part_6::*;
#[path = "parts/computer/refused_group_7.rs"]
mod part_7;
#[path = "parts/computer/refused_act_group_8.rs"]
mod part_8;
use part_8::*;
#[path = "parts/computer/of_group_9.rs"]
mod part_9;
#[path = "parts/computer/Admitted_group_10.rs"]
mod part_10;
use part_10::*;
#[path = "parts/computer/ComputerService_group_11.rs"]
mod part_11;
pub use part_11::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/DesktopUi_group_12.rs"]
mod part_12;
#[cfg(feature = "tauri-runtime")]
use part_12::*;
#[path = "parts/computer/ComputerHost_group_13.rs"]
mod part_13;
pub use part_13::*;
#[path = "parts/computer/spawn_task_group_14.rs"]
mod part_14;
use part_14::*;
#[path = "parts/computer/tools_enabled_group_15.rs"]
mod part_15;
#[path = "parts/computer/owned_clipboard_group_16.rs"]
mod part_16;
#[path = "parts/computer/share_screen_unless_stopped_group_17.rs"]
mod part_17;
#[path = "parts/computer/begin_group_18.rs"]
mod part_18;
#[path = "parts/computer/capture_inner_group_19.rs"]
mod part_19;
#[path = "parts/computer/verify_inner_group_20.rs"]
mod part_20;
#[path = "parts/computer/act_on_screen_group_21.rs"]
mod part_21;
#[path = "parts/computer/agent_clipboard_group_22.rs"]
mod part_22;
#[path = "parts/computer/Press_group_23.rs"]
mod part_23;
use part_23::*;
#[path = "parts/computer/length_group_24.rs"]
mod part_24;
#[path = "parts/computer/McpComputerTools_group_25.rs"]
mod part_25;
pub use part_25::*;
#[path = "parts/computer/new_group_26.rs"]
mod part_26;
#[path = "parts/computer/ComputerStatus_group_27.rs"]
mod part_27;
pub use part_27::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/service_group_28.rs"]
mod part_28;
#[cfg(feature = "tauri-runtime")]
use part_28::*;
#[path = "parts/computer/backend_error_group_29.rs"]
mod part_29;
use part_29::*;
#[path = "parts/computer/computer_status_core_group_30.rs"]
mod part_30;
pub use part_30::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_status_group_31.rs"]
mod part_31;
#[cfg(feature = "tauri-runtime")]
pub use part_31::*;
#[path = "parts/computer/PermissionRequestResult_group_32.rs"]
mod part_32;
pub use part_32::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_request_permission_group_33.rs"]
mod part_33;
#[cfg(feature = "tauri-runtime")]
pub use part_33::*;
#[path = "parts/computer/permission_settings_url_group_34.rs"]
mod part_34;
pub use part_34::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_open_permission_settings_group_35.rs"]
mod part_35;
#[cfg(feature = "tauri-runtime")]
pub use part_35::*;
#[path = "parts/computer/computer_list_shareable_windows_core_group_36.rs"]
mod part_36;
pub use part_36::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_list_shareable_windows_group_37.rs"]
mod part_37;
#[cfg(feature = "tauri-runtime")]
pub use part_37::*;
#[path = "parts/computer/computer_window_thumbnail_core_group_38.rs"]
mod part_38;
pub use part_38::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_window_thumbnail_group_39.rs"]
mod part_39;
#[cfg(feature = "tauri-runtime")]
pub use part_39::*;
#[path = "parts/computer/computer_share_window_core_group_40.rs"]
mod part_40;
pub use part_40::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_share_window_group_41.rs"]
mod part_41;
#[cfg(feature = "tauri-runtime")]
pub use part_41::*;
#[path = "parts/computer/ShareManyResult_group_42.rs"]
mod part_42;
pub use part_42::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_share_windows_group_43.rs"]
mod part_43;
#[cfg(feature = "tauri-runtime")]
pub use part_43::*;
#[path = "parts/computer/computer_shared_state_core_group_44.rs"]
mod part_44;
pub use part_44::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_shared_state_group_45.rs"]
mod part_45;
#[cfg(feature = "tauri-runtime")]
pub use part_45::*;
#[path = "parts/computer/SharedState_group_46.rs"]
mod part_46;
pub use part_46::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_share_screen_group_47.rs"]
mod part_47;
#[cfg(feature = "tauri-runtime")]
pub use part_47::*;
#[path = "parts/computer/computer_share_app_core_group_48.rs"]
mod part_48;
pub use part_48::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_share_app_group_49.rs"]
mod part_49;
#[cfg(feature = "tauri-runtime")]
pub use part_49::*;
#[path = "parts/computer/computer_revoke_all_core_group_50.rs"]
mod part_50;
pub use part_50::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_revoke_all_group_51.rs"]
mod part_51;
#[cfg(feature = "tauri-runtime")]
pub use part_51::*;
#[path = "parts/computer/computer_stop_core_group_52.rs"]
mod part_52;
pub use part_52::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_stop_group_53.rs"]
mod part_53;
#[cfg(feature = "tauri-runtime")]
pub use part_53::*;
#[path = "parts/computer/computer_stop_key_status_core_group_54.rs"]
mod part_54;
pub use part_54::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_stop_key_status_group_55.rs"]
mod part_55;
#[cfg(feature = "tauri-runtime")]
pub use part_55::*;
#[path = "parts/computer/computer_driver_info_core_group_56.rs"]
mod part_56;
pub use part_56::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_driver_info_group_57.rs"]
mod part_57;
#[cfg(feature = "tauri-runtime")]
pub use part_57::*;
#[path = "parts/computer/computer_driver_install_core_group_58.rs"]
mod part_58;
pub use part_58::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_driver_install_group_59.rs"]
mod part_59;
#[cfg(feature = "tauri-runtime")]
pub use part_59::*;
#[path = "parts/computer/computer_driver_uninstall_core_group_60.rs"]
mod part_60;
pub use part_60::*;
#[cfg(feature = "tauri-runtime")]
#[path = "parts/computer/computer_driver_uninstall_group_61.rs"]
mod part_61;
#[cfg(feature = "tauri-runtime")]
pub use part_61::*;
