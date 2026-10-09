//! iyw-claw's side of the helper: find it, launch it as its own TCC principal,
//! check that it is our helper, and talk to it.
//!
//! **Launch.** On macOS the helper is spawned with responsibility disclaimed,
//! so it is the TCC principal and iyw-claw is not — from a copy of its app
//! outside iyw-claw's bundle, since inside it macOS would charge its Screen
//! Recording to iyw-claw all the same (`helper_app`) — over a socketpair duplicated
//! onto its stdin and stdout — the only rendezvous there is, with no path in
//! the filesystem for another process to get to first. Its other descriptors
//! are closed on exec, its environment is a fixed few variables. Elsewhere it
//! is an ordinary child on pipes.
//!
//! **Check.** A release iyw-claw launches the helper under a launch requirement
//! (this build's Team ID and the helper's identifier), so the kernel runs
//! nothing else from that path — the bundle is writable by the user, and a
//! wrapper started in the helper's place could keep a copy of the socket. The
//! helper then speaks first; on macOS iyw-claw asks the kernel who is on the
//! other end of its socket and checks that process against the helper's
//! designated requirement (`IYW_CLAW_COMPUTER_HELPER_REQUIREMENT`), and every
//! later frame against the process it verified. A development iyw-claw checks
//! instead that the helper was built from its own sources (the fingerprint
//! `build.rs` compiles into both): `pnpm tauri dev` rebuilds only iyw-claw after
//! an edit — and not the helper at all under `IYW_CLAW_SKIP_SIDECAR=1` — and a
//! helper left over would answer with the old code.
//!
//! **Life.** One helper per iyw-claw, started on first use, restarted on the next
//! call after it dies, stopped when computer use is switched off — and not
//! started again until it is switched back on, whatever call was already on
//! its way. It exits on its own when iyw-claw does: its stdin closes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader};
use tokio::sync::{oneshot, watch, Mutex};

use super::backend::{
    ActRefusal, BackendError, BackendState, BackendStatus, ComputerBackend, SnapshotOptions,
};
use super::driver;
use super::protocol::{
    read_frame, write_frame, ClipboardUse, HelperError, HelperErrorCode, HelperMessage, HelperOp,
    HelperReply, HelperRequest, InstalledApp, OsPermission, PeerCheck, PermissionAsked,
    PermissionReport, ProcessRun, RawAct, RawApp, RawCapture, RawClipboard, RawLaunch, RawSnapshot,
    RawVerify, RawWindow, ScreenGeometry, ScreenRules, WindowAction, PROTOCOL_VERSION,
    SOURCE_FINGERPRINT, STOP_ALL,
};
use super::types::{ActDelivery, VerifyRequest};

#[path = "parts/local/HELPER_REQUIREMENT_group_1.rs"]
mod part_1;
pub use part_1::*;

// A release build pins both or neither: a requirement checked after launch
// without the launch requirement would let a wrapper run first.
const _: () = assert!(
    HELPER_REQUIREMENT.is_some() == HELPER_TEAM_ID.is_some(),
    "IYW_CLAW_COMPUTER_HELPER_REQUIREMENT and IYW_CLAW_COMPUTER_TEAM_ID are set together"
);

#[path = "parts/local/WRITE_TIMEOUT_group_2.rs"]
mod part_2;
use part_2::*;

#[cfg(target_os = "macos")]
#[path = "parts/local/PERMISSION_ASK_TIMEOUT_group_3.rs"]
mod part_3;
#[cfg(target_os = "macos")]
use part_3::*;

#[path = "parts/local/helper_file_name_group_4.rs"]
mod part_4;
pub use part_4::*;

#[path = "parts/local/helper_for_group_5.rs"]
mod part_5;
use part_5::*;

#[cfg(target_os = "macos")]
#[path = "parts/local/copy_to_run_group_6.rs"]
mod part_6;
#[cfg(target_os = "macos")]
use part_6::*;

#[path = "parts/local/alone_in_its_app_group_7.rs"]
mod part_7;
use part_7::*;

#[path = "parts/local/helper_to_reveal_group_8.rs"]
mod part_8;
pub use part_8::*;

#[path = "parts/local/app_of_group_9.rs"]
mod part_9;
use part_9::*;

#[path = "parts/local/exits_within_group_10.rs"]
mod part_10;

#[path = "parts/local/Slot_group_11.rs"]
mod part_11;
use part_11::*;

#[path = "parts/local/LocalBackend_group_12.rs"]
mod part_12;
pub use part_12::*;

#[path = "parts/local/new_group_13.rs"]
mod part_13;

#[path = "parts/local/status_group_14.rs"]
mod part_14;
#[path = "parts/local/act_screen_group_15.rs"]
mod part_15;

#[path = "parts/local/Io_group_16.rs"]
mod part_16;
use part_16::*;

#[cfg(target_os = "macos")]
#[path = "parts/local/spawn_helper_group_17.rs"]
mod part_17;
#[cfg(target_os = "macos")]
use part_17::*;

#[cfg(not(target_os = "macos"))]
#[path = "parts/local/spawn_helper_group_18.rs"]
mod part_18;
#[cfg(not(target_os = "macos"))]
use part_18::*;

#[path = "parts/local/VerifiedPeer_group_19.rs"]
mod part_19;
use part_19::*;

#[path = "parts/local/still_peer_group_20.rs"]
mod part_20;

#[cfg(target_os = "macos")]
#[path = "parts/local/check_helper_group_21.rs"]
mod part_21;
#[cfg(target_os = "macos")]
use part_21::*;

#[cfg(not(target_os = "macos"))]
#[path = "parts/local/check_helper_group_22.rs"]
mod part_22;
#[cfg(not(target_os = "macos"))]
use part_22::*;

#[path = "parts/local/forward_stderr_group_23.rs"]
mod part_23;
use part_23::*;

#[path = "parts/local/backend_trait.rs"]
mod backend_trait;

#[path = "parts/local/stop_operations.rs"]
mod stop_operations;
