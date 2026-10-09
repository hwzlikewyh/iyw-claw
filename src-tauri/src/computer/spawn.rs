//! `posix_spawn` with the two attributes computer use depends on, and a child
//! handle that cannot signal a recycled pid.
//!
//! * **Disclaim** (`responsibility_spawnattrs_setdisclaim`) makes the child
//!   its own TCC responsible process. iyw-claw launches the helper this way, so
//!   the permissions the user grants land on the helper and not on iyw-claw —
//!   where every agent's shell would inherit them. It is exported by
//!   libSystem but absent from the public headers, so it is looked up at run
//!   time; where it is missing, launching the helper fails rather than
//!   quietly charging its permissions to iyw-claw.
//! * **A launch requirement** (see [`super::launch_req`]) has the kernel
//!   refuse any image that is not the one named, at `exec`, before it runs.
//!   This is what holds a spawn from a user-writable path to the pinned build:
//!   the file can be swapped after it was checked, but not past this.
//! * **Start suspended** (`POSIX_SPAWN_START_SUSPENDED`) lets the helper look
//!   at the image the kernel has just mapped before resuming it, and kill it
//!   instead. It is a second look, not a gate: a child started suspended is
//!   stopped by a signal, and any process of the same user may send the
//!   `SIGCONT` that starts it.
//!
//! Every spawn also sets `POSIX_SPAWN_CLOEXEC_DEFAULT`: the child gets exactly
//! the three descriptors named here and nothing else this process has open.

use std::ffi::{c_int, CString};
use std::os::fd::RawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[path = "parts/spawn/ChildFd_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/spawn/DisclaimFn_group_2.rs"]
mod part_2;
use part_2::*;

#[path = "parts/spawn/can_disclaim_group_3.rs"]
mod part_3;
pub use part_3::*;

#[path = "parts/spawn/cstring_group_4.rs"]
mod part_4;
use part_4::*;

#[path = "parts/spawn/spawn_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/spawn/check_group_6.rs"]
mod part_6;
use part_6::*;

#[path = "parts/spawn/Child_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/spawn/ExitWatch_group_8.rs"]
mod part_8;
use part_8::*;

#[path = "parts/spawn/register_group_9.rs"]
mod part_9;

#[path = "parts/spawn/discard_group_10.rs"]
mod part_10;
use part_10::*;

#[path = "parts/spawn/new_group_11.rs"]
mod part_11;
