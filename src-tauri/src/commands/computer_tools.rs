//! The computer-use settings: whether an agent may see the desktop at all,
//! how long a shared window stays shared unused, which applications can
//! never be shared, the shortcut that stops every agent at once, whether
//! the strip with Stop on it floats above every window while anything is
//! shared, and whether an agent may have a window brought to the front to
//! act on it — and, if so, whether that is how every action goes unless it
//! asks otherwise.
//!
//! Separate from `commands::computer`, which is the desktop feature itself and
//! exists only in the desktop build: these switches are read by the shared
//! 主进程 HTTP MCP 使用同一个配置；服务端只在显式启用桌面功能时发布工具。
//!
//! **Off by default.** It hands an agent a view of the user's screen.
//! Sharing an individual window is a second decision on top of it
//! (`crate::computer::agent`); this switch only decides whether the tools
//! exist. Switching it off ends every grant and stops the helper — that part
//! lives with the desktop's computer service, which watches the runtime
//! config.

use std::time::Duration;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::acp::computer_tools::{ComputerToolsConfig, ComputerToolsRuntimeConfig};
use crate::app_error::AppCommandError;
use crate::computer::agent::{default_blocklist, is_default_key, DefaultBlockView};
use crate::computer::keys::Platform;
use crate::computer::stop_shortcut::StopShortcut;
use crate::computer::types::ActDelivery;
use crate::db::service::app_metadata_service;
use crate::web::event_bridge::{emit_event, EventEmitter, COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT};

async fn sync_computer_skill(conn: &DatabaseConnection, enabled: bool) {
    match crate::commands::managed_skills::set_global_enabled_core(
        conn,
        crate::commands::managed_skills::ManagedSkillFamily::ComputerUse,
        enabled,
    )
    .await
    {
        Ok(report) if report.results.iter().all(|result| result.ok) => {}
        Ok(report) => tracing::warn!(
            failed = report.results.iter().filter(|result| !result.ok).count(),
            "[computer] native skill publication incomplete"
        ),
        Err(error) => {
            tracing::warn!(error = %error.message, "[computer] native skill policy synchronization failed")
        }
    }
}

#[path = "parts/computer_tools/KEY_COMPUTER_TOOLS_ENABLED_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/computer_tools/default_ttl_group_2.rs"]
mod part_2;
use part_2::*;

#[path = "parts/computer_tools/default_group_3.rs"]
mod part_3;

#[path = "parts/computer_tools/stored_delivery_group_4.rs"]
mod part_4;
use part_4::*;

#[path = "parts/computer_tools/load_computer_tools_settings_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/computer_tools/COMPUTER_TOOLS_WRITE_LOCK_group_6.rs"]
mod part_6;
use part_6::*;

#[path = "parts/computer_tools/set_computer_tools_enabled_core_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/computer_tools/set_computer_tools_preferences_group_8.rs"]
mod part_8;
pub use part_8::*;
