//! A minimal MCP client for the driver's stdio: `initialize`, then
//! `tools/call`, multiplexed by JSON-RPC id.
//!
//! Minimal on purpose. The driver is the only server this ever talks to, and
//! the helper only ever calls the handful of tools `ops` names; everything
//! MCP offers beyond that (resources, prompts, sampling, subscriptions) is
//! something the helper has no use for and so does not implement. A request
//! the driver sends *to* the helper gets "method not found" rather than
//! silence, so the driver is never left waiting on it.
//!
//! **One call at a time.** The driver's stdio server reads a request, runs
//! it, answers, and only then reads the next; a second request sent while one
//! runs just waits in the pipe — and, past what the pipe holds, stalls the
//! write. So calls take turns here, which costs nothing the driver would have
//! given: the one on the wire is the only one it is working on, a write can
//! only stall on a driver that has stopped reading, and a call that runs past
//! its time leaves the driver busy with something nobody is waiting for — so
//! it is given up on, and the next call starts another.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, watch};

#[path = "parts/mcp/MCP_PROTOCOL_VERSION_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/mcp/MAX_LINE_BYTES_group_2.rs"]
mod part_2;
use part_2::*;

#[path = "parts/mcp/McpError_group_3.rs"]
mod part_3;
pub use part_3::*;

#[path = "parts/mcp/fmt_group_4.rs"]
mod part_4;

#[path = "parts/mcp/ToolCallResult_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/mcp/from_value_group_6.rs"]
mod part_6;

#[path = "parts/mcp/Pending_group_7.rs"]
mod part_7;
use part_7::*;

#[path = "parts/mcp/McpClient_group_8.rs"]
mod part_8;
pub use part_8::*;

#[path = "parts/mcp/start_group_9.rs"]
mod part_9;

#[path = "parts/mcp/reply_value_group_10.rs"]
mod part_10;
use part_10::*;
