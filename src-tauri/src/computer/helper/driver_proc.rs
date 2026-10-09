//! Launch the driver, hold it to the pins, and talk to it.
//!
//! On macOS the driver runs with the helper's TCC grants, which makes three
//! things about how it is started part of the security boundary:
//!
//! * **What runs.** The file is hashed against the pinned digest (which
//!   catches a damaged download early), then spawned under a launch
//!   requirement naming trycua's Team ID, the driver's identifier and the
//!   pinned cdhashes — the kernel kills any other image at `exec`, before it
//!   runs, which is what holds even if the file is swapped after it was
//!   hashed. The child starts suspended, and the *running image* is checked
//!   again (designated requirement plus cdhash, hardened runtime, the exact
//!   entitlement list) before it is resumed. Where the kernel cannot take a
//!   launch requirement (macOS before 14.4) the driver is not started at
//!   all: the suspended check alone is not a gate, because any process of
//!   the user's may resume a suspended child.
//! * **What it finds on `PATH`.** The driver runs `plutil`, `ps` and
//!   `osascript` by bare name. iyw-claw's own `PATH` carries directories the user
//!   (and so any agent) can write — the login shell's, `~/.iyw-claw/npm-global/bin`
//!   first of all, Homebrew's — and a fake `ps` in one of them would run with
//!   the helper's grants the next time the driver lists applications. So the
//!   driver gets a fixed system `PATH` and nothing else from this process's
//!   environment.
//! * **What else it inherits.** Nothing: exactly three descriptors, a fresh
//!   home directory per launch (the driver reads its configuration from
//!   there), telemetry and the update check off, and embedded mode, which
//!   only ever removes capability (no disclaim re-exec, no relaunch as its
//!   own app, no permission prompts).
//!
//! On Windows and Linux the same environment rules apply for the same
//! reasons, and the file is hashed before every launch; there is no running
//! image to check, and no TCC grant for a replacement to borrow.
//!
//! The same pinned build also answers the helper's permission questions on
//! macOS ([`probe_permissions`]): a copy started only to report what TCC says
//! and exit, under the same launch requirement and checks.
//!
//! Two settings in that home decide how the driver behaves over a long life:
//!
//! * **Its session never idles out.** The driver ends a caller's session
//!   after five idle minutes, and with it every snapshot the session holds —
//!   every ref, and the capture a point is aimed by — and the helper's
//!   driver lives for as long as computer use is on.
//! * **It captures windows at their own size.** The driver converts a click's
//!   pixel coordinates by the scale of the capture in the window's latest
//!   snapshot, at whatever size it was taken — one taken smaller in between
//!   would move every later click. At full size there is no scale to
//!   remember: the helper shrinks images itself, asks the driver for no other
//!   size, and a point is always the window's own pixel.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use super::mcp::{McpClient, McpError, ToolCallResult};
use crate::computer::driver::{self, DriverArtifact};
#[cfg(target_os = "macos")]
use crate::computer::protocol::PermissionReport;
use crate::computer::protocol::{HelperError, HelperErrorCode};

#[path = "parts/driver_proc/INITIALIZE_TIMEOUT_group_1.rs"]
mod part_1;
use part_1::*;

#[cfg(target_os = "macos")]
#[path = "parts/driver_proc/PERMISSION_PROBE_ARG_group_2.rs"]
mod part_2;
#[cfg(target_os = "macos")]
use part_2::*;

#[path = "parts/driver_proc/helper_data_dir_group_3.rs"]
mod part_3;
pub use part_3::*;

#[cfg(unix)]
#[path = "parts/driver_proc/account_home_dir_group_4.rs"]
mod part_4;
#[cfg(unix)]
use part_4::*;

#[path = "parts/driver_proc/driver_environment_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/driver_proc/driver_environment_with_group_6.rs"]
mod part_6;
use part_6::*;

#[path = "parts/driver_proc/driver_requirement_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/driver_proc/ChildProc_group_8.rs"]
mod part_8;
use part_8::*;

#[path = "parts/driver_proc/DriverProc_group_9.rs"]
mod part_9;
pub use part_9::*;

#[path = "parts/driver_proc/launch_group_10.rs"]
mod part_10;
#[path = "parts/driver_proc/spawn_group_11.rs"]
mod part_11;

#[cfg(target_os = "macos")]
#[path = "parts/driver_proc/probe_permissions_group_12.rs"]
mod part_12;
#[cfg(target_os = "macos")]
pub use part_12::*;

#[cfg(target_os = "macos")]
#[path = "parts/driver_proc/driver_launch_requirement_group_13.rs"]
mod part_13;
#[cfg(target_os = "macos")]
use part_13::*;

#[path = "parts/driver_proc/forward_stderr_group_14.rs"]
mod part_14;
use part_14::*;
