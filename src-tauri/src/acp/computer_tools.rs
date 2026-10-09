//! Listener-facing access for computer use (`computer_list_apps`,
//! `computer_list_windows`, `computer_screenshot`, `computer_snapshot`,
//! `computer_verify`, and the actions `computer_click`, `computer_drag`,
//! `computer_scroll`, `computer_type`, `computer_press_key`,
//! `computer_hold_key`, `computer_set_value`, `computer_restore`,
//! `computer_invoke_menu`) carried by iyw-claw-mcp.
//!
//! The same split as the browser tools: nothing here decides whether a window
//! may be read. That is `crate::computer::agent` and the target table, and it
//! is enforced inside `commands::computer`, which the production impl calls —
//! so an MCP read passes the same grant check, and leaves the same line on the
//! panel's activity list, as any other.
//!
//! What this module owns is the shape of the answer — a refusal is a value,
//! not a transport error, so the agent can relay "ask the user to share that
//! window" instead of losing its turn — the words of each refusal, each of
//! which says whether trying again can help (a model takes that sentence
//! literally), and the answer where there is no
//! desktop at all ([`NoComputerDesktop`]): server mode, where the group is not
//! advertised in the first place.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::computer::types::{
    ActDelivery, ActReport, AgentAppSummary, AgentScreen, AgentWindowSummary, ComputerActRequest,
    VerifyOutcome, VerifyRequest, WindowCapture, WindowSnapshot,
};

#[path = "parts/computer_tools/ERROR_UNAVAILABLE_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/computer_tools/SHARE_HOW_group_2.rs"]
mod part_2;
use part_2::*;

#[path = "parts/computer_tools/ComputerAppsOutcome_group_3.rs"]
mod part_3;
pub use part_3::*;

#[path = "parts/computer_tools/refused_group_4.rs"]
mod part_4;

#[path = "parts/computer_tools/ComputerLaunchOutcome_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/computer_tools/refused_group_6.rs"]
mod part_6;

#[path = "parts/computer_tools/ClipboardOp_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/computer_tools/refused_group_8.rs"]
mod part_8;

#[path = "parts/computer_tools/ComputerWindowsOutcome_group_9.rs"]
mod part_9;
pub use part_9::*;

#[path = "parts/computer_tools/refused_group_10.rs"]
mod part_10;

#[path = "parts/computer_tools/InputPolicy_group_11.rs"]
mod part_11;
pub use part_11::*;

#[path = "parts/computer_tools/of_group_12.rs"]
mod part_12;

#[path = "parts/computer_tools/grant_required_note_group_13.rs"]
mod part_13;
pub use part_13::*;

#[path = "parts/computer_tools/no_pointing_note_group_14.rs"]
mod part_14;
pub use part_14::*;

#[path = "parts/computer_tools/done_group_15.rs"]
mod part_15;

#[path = "parts/computer_tools/ComputerCaptureOutcome_group_16.rs"]
mod part_16;
pub use part_16::*;

#[path = "parts/computer_tools/image_group_17.rs"]
mod part_17;

#[path = "parts/computer_tools/SnapshotRequest_group_18.rs"]
mod part_18;
pub use part_18::*;

#[path = "parts/computer_tools/tree_group_19.rs"]
mod part_19;

#[path = "parts/computer_tools/ComputerVerifyOutcome_group_20.rs"]
mod part_20;
pub use part_20::*;

#[path = "parts/computer_tools/verdict_group_21.rs"]
mod part_21;

/// Listener-facing access to computer use. The production impl
/// (`crate::commands::computer::McpComputerTools`) exists only in the desktop
/// build; server mode and tests use [`NoComputerDesktop`].
#[async_trait]
pub trait ComputerToolAccess: Send + Sync {
    /// The running applications.
    async fn list_apps(&self) -> ComputerAppsOutcome;

    /// Every normal window, or `pid`'s only.
    async fn list_windows(&self, pid: Option<u32>) -> ComputerWindowsOutcome;

    /// A screenshot of one shared window.
    async fn capture(&self, target_id: &str, max_dimension: Option<u32>) -> ComputerCaptureOutcome;

    /// The accessibility tree of one shared window.
    async fn snapshot(&self, target_id: &str, request: SnapshotRequest) -> ComputerSnapshotOutcome;

    /// Check predicates against one shared window.
    async fn verify(&self, target_id: &str, request: VerifyRequest) -> ComputerVerifyOutcome;

    /// Act on one window shared for control — brought to the front for it
    /// or not as `delivery` asks, or as the person set it when it does not.
    async fn act(
        &self,
        target_id: &str,
        request: ComputerActRequest,
        delivery: Option<ActDelivery>,
    ) -> ComputerActOutcome;

    /// Start an installed application — by its key, or by its name — in the
    /// background. Its windows are not shared by it.
    async fn launch_app(&self, name: Option<String>, key: Option<String>) -> ComputerLaunchOutcome;

    /// Read back what the agent put on the clipboard, or put text there.
    async fn clipboard(&self, op: ClipboardOp) -> ComputerClipboardOutcome;
}

#[path = "parts/computer_tools/NoComputerDesktop_group_22.rs"]
mod part_22;
pub use part_22::*;

#[path = "parts/computer_tools/list_apps_group_23.rs"]
mod part_23;

#[path = "parts/computer_tools/ComputerToolsConfig_group_24.rs"]
mod part_24;
pub use part_24::*;

#[path = "parts/computer_tools/default_group_25.rs"]
mod part_25;

#[path = "parts/computer_tools/ComputerToolsRuntimeConfig_group_26.rs"]
mod part_26;
pub use part_26::*;

#[path = "parts/computer_tools/ChangeHook_group_27.rs"]
mod part_27;
use part_27::*;

#[path = "parts/computer_tools/default_group_28.rs"]
mod part_28;
