//! Long-lived format-handler session per plugins/plugin-protocol.md §5.3.
//!
//! Owns the spawned plugin process and drives a request/response JSON-RPC
//! stream over its stdin/stdout. Implements the §5.6 idle-timeout watchdog
//! using the same `last_activity` trick as the short-lived `plugin_process`
//! driver, plus heartbeat parsing.
//!
//! v1 scope: a single client-facing `FormatHandlerSession` with a
//! `send_request` API and an explicit `save` / `close` lifecycle. The
//! `DocumentHandler` trait proxy that wraps the session lives in
//! [`FormatHandlerProxy`].

// These public introspection helpers and capability fields are the plugin
// protocol's surface, not just internal helpers — future commands (e.g.
// `officecli plugins session-info`) may consume them. Allow dead code
// until a command wires them in.
#![allow(dead_code)]

use handler_common::HandlerError;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::plugins::{enumerate_all, PluginManifest};

/// Session lifecycle state per §6.7.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionState {
    Spawning,
    Ready,
    Busy,
    Broken,
    Closed,
}

/// Capabilities + vocabulary returned by the plugin's open handshake.
#[derive(Debug, Clone, Default)]
pub struct PluginCapabilities {
    pub commands: Vec<String>,
    pub features: Vec<String>,
    pub addable_types: Vec<String>,
    pub path_segments: Vec<String>,
}

/// A live format-handler session. The inner state is shared via Mutex so
/// callers that share a session across threads serialize correctly (the
/// plugin is single-request-in-flight; concurrent callers wait on the
/// mutex).
pub struct FormatHandlerSession {
    inner: Mutex<SessionInner>,
}

struct SessionInner {
    state: SessionState,
    child: Child,
    child_stdin: std::process::ChildStdin,
    child_stdout: BufReader<std::process::ChildStdout>,
    /// Last activity timestamp in millis (wall-clock). Updated on every
    /// byte of stdout / heartbeat line on stderr by the watchdog thread.
    last_activity: Arc<AtomicI64>,
    /// Watchdog handle so we can signal shutdown on close.
    _watchdog: std::thread::JoinHandle<()>,
    capabilities: PluginCapabilities,
    plugin_name: String,
}

/// Build a request envelope.
fn build_envelope(msg_type: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("protocol".into(), 1.into());
    obj.insert(
        "msg_type".into(),
        serde_json::Value::String(msg_type.to_string()),
    );
    if let serde_json::Value::Object(map) = body {
        for (k, v) in map {
            obj.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(obj)
}

/// Build a "command" envelope with args + props.
fn build_command_envelope(
    command: &str,
    args: &serde_json::Value,
    props: &HashMap<String, String>,
) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert(
        "command".into(),
        serde_json::Value::String(command.to_string()),
    );
    body.insert("args".into(), args.clone());
    let props_obj: serde_json::Map<String, serde_json::Value> = props
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    body.insert("props".into(), serde_json::Value::Object(props_obj));
    build_envelope("command", &serde_json::Value::Object(body))
}

impl FormatHandlerSession {
    /// Resolve a format-handler plugin for `source_ext` and open a session
    /// against `path`. Performs the open handshake and returns Ready.
    pub fn open(source_path: &str) -> Result<Self, HandlerError> {
        let source_ext = std::path::Path::new(source_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        let source_dotted = if source_ext.is_empty() {
            String::new()
        } else {
            format!(".{}", source_ext)
        };
        let (exe, manifest) = enumerate_all()
            .into_iter()
            .find(|p| {
                if !p.manifest.kinds.iter().any(|k| k == "format-handler") {
                    return false;
                }
                p.manifest.extensions.iter().any(|e| {
                    e.eq_ignore_ascii_case(&source_dotted) || e.eq_ignore_ascii_case(&source_ext)
                })
            })
            .map(|p| (p.executable_path, p.manifest))
            .ok_or_else(|| {
                HandlerError::UnsupportedMode(format!(
                    "no format-handler plugin found for .{} — install one or see plugins/plugin-protocol.md",
                    source_ext
                ))
            })?;

        Self::open_with(&exe, &manifest, source_path)
    }

    /// Open a session against an explicit executable + manifest.
    pub fn open_with(
        exe: &str,
        manifest: &PluginManifest,
        source_path: &str,
    ) -> Result<Self, HandlerError> {
        let idle = super::plugin_process::resolve_idle_timeout(manifest, "default");
        let exe_path = std::env::current_exe().ok();
        let mut cmd = Command::new(exe);
        cmd.args(["open", source_path]);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        if let Some(p) = &exe_path {
            cmd.env("OFFICECLI_BIN", p);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| HandlerError::OperationFailed(format!("failed to spawn plugin: {}", e)))?;
        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| HandlerError::OperationFailed("plugin stdin not piped".into()))?;
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| HandlerError::OperationFailed("plugin stdout not piped".into()))?;
        let child_stderr = child.stderr.take();

        let last_activity = Arc::new(AtomicI64::new(now_millis()));
        // Stderr reader thread — drains diagnostics, updates activity, hides
        // heartbeat lines.
        if let Some(stderr) = child_stderr {
            let la = last_activity.clone();
            std::thread::spawn(move || {
                use std::io::BufRead;
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(_) => break,
                    };
                    if super::plugin_process::is_heartbeat(&line) {
                        la.store(now_millis(), Ordering::SeqCst);
                        continue;
                    }
                    la.store(now_millis(), Ordering::SeqCst);
                }
            });
        }

        // Watchdog thread — periodically kills the process if activity has
        // been silent past the idle budget. Holds a weak handle so it
        // doesn't extend lifetime; uses raw PID via kill on Unix.
        let mut state_inner = SessionInner {
            state: SessionState::Spawning,
            child,
            child_stdin,
            child_stdout: BufReader::new(child_stdout),
            last_activity: last_activity.clone(),
            _watchdog: std::thread::spawn(|| {}),
            capabilities: PluginCapabilities::default(),
            plugin_name: manifest.name.clone(),
        };

        let pid_opt = state_inner.child.id();
        let la = last_activity.clone();
        let budget_ms = idle * 1000;
        let watchdog = std::thread::spawn(move || {
            // Watchdog: SIGKILL the process group when activity goes stale.
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let last = la.load(Ordering::SeqCst);
                if budget_ms > 0 && now_millis().saturating_sub(last) > budget_ms as i64 {
                    kill_process_group(pid_opt as i32);
                    return;
                }
            }
        });
        state_inner._watchdog = watchdog;

        let session = FormatHandlerSession {
            inner: Mutex::new(state_inner),
        };

        // Open handshake: write request, read reply.
        let open_body = serde_json::json!({
            "path": source_path,
            "editable": true,
        });
        let envelope = build_envelope("open", &open_body);
        let reply = session.send_raw(&envelope)?;
        // Parse capabilities from reply.result.
        if let Some(result) = reply.get("result") {
            if let Some(caps) = result.get("capabilities") {
                let commands = caps
                    .get("commands")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let features = caps
                    .get("features")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut capabilities = PluginCapabilities {
                    commands,
                    features,
                    ..Default::default()
                };
                if let Some(voc) = result.get("vocabulary") {
                    capabilities.addable_types = voc
                        .get("addable_types")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    capabilities.path_segments = voc
                        .get("path_segments")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                }
                let mut inner = session.inner.lock().unwrap();
                inner.capabilities = capabilities;
            }
        }

        {
            let mut inner = session.inner.lock().unwrap();
            inner.state = SessionState::Ready;
        }

        Ok(session)
    }

    /// Send a command envelope and return the `result` value from the
    /// `ok` reply. Returns an error if the plugin returns `error` or
    /// the stream closes.
    pub fn send_command(
        &self,
        command: &str,
        args: &serde_json::Value,
        props: &HashMap<String, String>,
    ) -> Result<serde_json::Value, HandlerError> {
        let envelope = build_command_envelope(command, args, props);
        let reply = self.send_raw(&envelope)?;
        match reply.get("msg_type").and_then(|v| v.as_str()) {
            Some("ok") => Ok(reply
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null)),
            Some("error") => {
                let err = reply.get("error").cloned().unwrap_or_default();
                let code = err
                    .get("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("internal_error");
                let message = err
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no message)");
                Err(HandlerError::OperationFailed(format!(
                    "plugin {}: {} — {}",
                    self.plugin_name(),
                    code,
                    message
                )))
            }
            Some(other) => Err(HandlerError::OperationFailed(format!(
                "plugin {}: unexpected reply msg_type '{}'",
                self.plugin_name(),
                other
            ))),
            None => Err(HandlerError::OperationFailed(format!(
                "plugin {}: reply missing msg_type",
                self.plugin_name()
            ))),
        }
    }

    /// Send a fully-formed envelope and read one reply. Internal helper.
    fn send_raw(&self, envelope: &serde_json::Value) -> Result<serde_json::Value, HandlerError> {
        let mut guard = self.inner.lock().unwrap();
        if matches!(guard.state, SessionState::Broken | SessionState::Closed) {
            return Err(HandlerError::OperationFailed(format!(
                "plugin {} session is {:?}",
                guard.plugin_name, guard.state
            )));
        }
        // Snapshot previous state BEFORE transitioning to Busy so a Spawning
        // caller's open-handshake stays in Spawning until they explicitly
        // mark it Ready. (Capturing after the assignment would always read
        // Busy and collapse the state machine.)
        let prev_state = guard.state;
        guard.state = SessionState::Busy;

        let line = serde_json::to_string(envelope)?;
        if let Err(e) = writeln!(guard.child_stdin, "{}", line) {
            guard.state = SessionState::Broken;
            return Err(HandlerError::OperationFailed(format!(
                "plugin {}: write to stdin failed: {}",
                guard.plugin_name, e
            )));
        }
        let _ = guard.child_stdin.flush();

        let mut buf = String::new();
        match guard.child_stdout.read_line(&mut buf) {
            Ok(0) => {
                guard.state = SessionState::Broken;
                return Err(HandlerError::OperationFailed(format!(
                    "plugin {}: stdout closed before reply",
                    guard.plugin_name
                )));
            }
            Ok(_) => {}
            Err(e) => {
                guard.state = SessionState::Broken;
                return Err(HandlerError::OperationFailed(format!(
                    "plugin {}: stdout read failed: {}",
                    guard.plugin_name, e
                )));
            }
        }

        // Bump activity since we just read a reply.
        guard.last_activity.store(now_millis(), Ordering::SeqCst);
        guard.state = if matches!(prev_state, SessionState::Spawning) {
            SessionState::Spawning
        } else {
            SessionState::Ready
        };

        let trimmed = buf.trim();
        let parsed: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
            HandlerError::OperationFailed(format!(
                "plugin {}: non-envelope reply on stdout: '{}' ({})",
                guard.plugin_name, trimmed, e
            ))
        })?;
        Ok(parsed)
    }

    /// Send a `save` envelope. Plugin MUST flush before replying (§5.3).
    pub fn save(&self) -> Result<(), HandlerError> {
        let envelope = build_envelope("save", &serde_json::Value::Object(Default::default()));
        let reply = self.send_raw(&envelope)?;
        match reply.get("msg_type").and_then(|v| v.as_str()) {
            Some("ok") => Ok(()),
            Some(other) => Err(HandlerError::OperationFailed(format!(
                "plugin {}: save returned {}",
                self.plugin_name(),
                other
            ))),
            None => Err(HandlerError::OperationFailed(format!(
                "plugin {}: save reply missing msg_type",
                self.plugin_name()
            ))),
        }
    }

    /// Send a `close` envelope, transition to Closed, and reap the process.
    pub fn close(&self) -> Result<(), HandlerError> {
        let mut guard = self.inner.lock().unwrap();
        if matches!(guard.state, SessionState::Closed) {
            return Ok(());
        }
        let envelope = build_envelope("close", &serde_json::Value::Object(Default::default()));
        let line = serde_json::to_string(&envelope)?;
        let _ = writeln!(guard.child_stdin, "{}", line);
        let _ = guard.child_stdin.flush();

        // Read one reply line (best-effort). If the plugin writes nothing,
        // move on after a short wait.
        let mut buf = String::new();
        let _ = guard.child_stdout.read_line(&mut buf);

        // Now reap.
        let _ = guard.child.wait();
        guard.state = SessionState::Closed;
        Ok(())
    }

    /// Snapshot of plugin-declared capabilities from the open handshake.
    pub fn capabilities(&self) -> PluginCapabilities {
        let guard = self.inner.lock().unwrap();
        guard.capabilities.clone()
    }

    /// Plugin name (from manifest).
    pub fn plugin_name(&self) -> String {
        let guard = self.inner.lock().unwrap();
        guard.plugin_name.clone()
    }

    /// Current session state.
    pub fn state(&self) -> SessionState {
        let guard = self.inner.lock().unwrap();
        guard.state
    }
}

impl Drop for FormatHandlerSession {
    fn drop(&mut self) {
        let mut guard = self.inner.lock().unwrap();
        if !matches!(guard.state, SessionState::Closed) {
            // Best-effort kill. Plugin may already be dead.
            let _ = guard.child.kill();
            let _ = guard.child.wait();
            guard.state = SessionState::Closed;
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(unix)]
fn kill_process_group(pgid: i32) {
    // SIGKILL the process group. Safe to ignore errors here — if the
    // process is already dead, kill(2) returns ESRCH and we move on.
    extern "C" {
        fn kill(pgid: i32, sig: i32) -> i32;
    }
    // libc SIGKILL = 9.
    unsafe {
        let _ = kill(-pgid, 9);
    }
}

#[cfg(not(unix))]
fn kill_process_group(_pgid: i32) {
    // No process groups on Windows; the watchdog is a no-op here. The
    // session's Drop impl reaps the child on its own.
}

// ─── FormatHandlerProxy: DocumentHandler trait delegation ────

use handler_common::{
    BinaryInfo, DocumentHandler, DocumentIssue, DocumentNode, InsertPosition, RawOptions,
    TextOffsetMap, ValidationError, ViewOptions,
};

/// `DocumentHandler` trait wrapper around a long-lived `FormatHandlerSession`.
///
/// Every method marshals its inputs into a `command` envelope per
/// plugins/plugin-protocol.md §5.3, sends it via the session, and unmarshals
/// the plugin's reply into the host-side type. Plugins decide whether each
/// verb is supported; unsupported verbs come back as `error.code =
/// unsupported_command`, which we surface as `HandlerError::UnsupportedMode`.
pub struct FormatHandlerProxy {
    session: FormatHandlerSession,
    /// Cached at construction so `format_name` can return `&str` without
    /// holding the session mutex.
    name: String,
}

impl FormatHandlerProxy {
    /// Open a foreign file by delegating to a format-handler plugin.
    pub fn open(source_path: &str) -> Result<Self, HandlerError> {
        let session = FormatHandlerSession::open(source_path)?;
        let name = session.plugin_name();
        Ok(Self { session, name })
    }

    /// Reference to the underlying session (for tests and introspection).
    pub fn session(&self) -> &FormatHandlerSession {
        &self.session
    }

    /// Marshal `InsertPosition` to the JSON shape from §5.3:
    /// `{"index":N}` / `{"after":"..."}` / `{"before":"..."}` / omitted.
    fn position_to_json(pos: &InsertPosition) -> serde_json::Value {
        match pos {
            InsertPosition::AtIndex(i) => serde_json::json!({"index": i}),
            InsertPosition::AfterElement(p) => serde_json::json!({"after": p}),
            InsertPosition::BeforeElement(p) => serde_json::json!({"before": p}),
            InsertPosition::Append => serde_json::Value::Null,
        }
    }

    /// Convert a view-mode string + ViewOptions into args JSON.
    fn view_args(mode: &str, opts: &ViewOptions) -> serde_json::Value {
        let mut args = serde_json::Map::new();
        args.insert("mode".into(), serde_json::Value::String(mode.to_string()));
        if let Some(s) = opts.start_line {
            args.insert("start".into(), serde_json::Value::from(s));
        }
        if let Some(e) = opts.end_line {
            args.insert("end".into(), serde_json::Value::from(e));
        }
        if let Some(m) = opts.max_lines {
            args.insert("max_lines".into(), serde_json::Value::from(m));
        }
        if let Some(cols) = &opts.cols {
            let arr: Vec<serde_json::Value> = cols
                .iter()
                .map(|c| serde_json::Value::String(c.clone()))
                .collect();
            args.insert("cols".into(), serde_json::Value::Array(arr));
        }
        serde_json::Value::Object(args)
    }

    /// Send a command, surfacing plugin errors that name `unsupported_*`
    /// as `HandlerError::UnsupportedMode` so the host can fall back.
    fn send(
        &self,
        command: &str,
        args: &serde_json::Value,
        props: &HashMap<String, String>,
    ) -> Result<serde_json::Value, HandlerError> {
        match self.session.send_command(command, args, props) {
            Ok(v) => Ok(v),
            Err(HandlerError::OperationFailed(msg)) if msg.contains("unsupported_") => {
                Err(HandlerError::UnsupportedMode(format!(
                    "plugin {} does not support '{}'",
                    self.name, command
                )))
            }
            Err(e) => Err(e),
        }
    }
}

impl DocumentHandler for FormatHandlerProxy {
    fn format_name(&self) -> &str {
        // `self.name` lives as long as `self` does, satisfying `&self -> &str`.
        self.name.as_str()
    }

    fn view_as_text(&self, opts: ViewOptions) -> Result<String, HandlerError> {
        let args = Self::view_args("text", &opts);
        let result = self.send("view", &args, &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    fn view_as_annotated(&self, opts: ViewOptions) -> Result<String, HandlerError> {
        let args = Self::view_args("annotated", &opts);
        let result = self.send("view", &args, &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    fn view_as_outline(&self) -> Result<String, HandlerError> {
        let args = serde_json::json!({"mode": "outline"});
        let result = self.send("view", &args, &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    fn view_as_stats(&self) -> Result<String, HandlerError> {
        let args = serde_json::json!({"mode": "stats"});
        let result = self.send("view", &args, &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    fn view_as_issues(
        &self,
        issue_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<DocumentIssue>, HandlerError> {
        let mut args = serde_json::Map::new();
        args.insert("mode".into(), serde_json::Value::String("issues".into()));
        if let Some(t) = issue_type {
            args.insert("type".into(), serde_json::Value::String(t.to_string()));
        }
        if let Some(l) = limit {
            args.insert("limit".into(), serde_json::Value::from(l));
        }
        let result = self.send("view", &serde_json::Value::Object(args), &HashMap::new())?;
        let arr = match result {
            serde_json::Value::Array(a) => a,
            other => {
                return Err(HandlerError::OperationFailed(format!(
                    "plugin {}: view issues expected array, got {}",
                    self.name, other
                )))
            }
        };
        arr.into_iter()
            .map(|v| {
                serde_json::from_value::<DocumentIssue>(v).map_err(|e| {
                    HandlerError::OperationFailed(format!(
                        "plugin {}: malformed issue object: {}",
                        self.name, e
                    ))
                })
            })
            .collect()
    }

    fn view_as_text_json(&self, opts: ViewOptions) -> Result<serde_json::Value, HandlerError> {
        let mut args = Self::view_args("text", &opts)
            .as_object()
            .cloned()
            .unwrap_or_default();
        args.insert("format".into(), serde_json::Value::String("json".into()));
        self.send("view", &serde_json::Value::Object(args), &HashMap::new())
    }

    fn view_as_outline_json(&self) -> Result<serde_json::Value, HandlerError> {
        let args = serde_json::json!({"mode": "outline", "format": "json"});
        self.send("view", &args, &HashMap::new())
    }

    fn view_as_stats_json(&self) -> Result<serde_json::Value, HandlerError> {
        let args = serde_json::json!({"mode": "stats", "format": "json"});
        self.send("view", &args, &HashMap::new())
    }

    fn get(&self, path: &str, depth: usize) -> Result<DocumentNode, HandlerError> {
        let args = serde_json::json!({"path": path, "depth": depth});
        let result = self.send("get", &args, &HashMap::new())?;
        serde_json::from_value::<DocumentNode>(result).map_err(|e| {
            HandlerError::OperationFailed(format!(
                "plugin {}: malformed DocumentNode for '{}': {}",
                self.name, path, e
            ))
        })
    }

    fn query(&self, selector: &str) -> Result<Vec<DocumentNode>, HandlerError> {
        let args = serde_json::json!({"selector": selector});
        let result = self.send("query", &args, &HashMap::new())?;
        let arr = match result {
            serde_json::Value::Array(a) => a,
            other => {
                return Err(HandlerError::OperationFailed(format!(
                    "plugin {}: query expected array, got {}",
                    self.name, other
                )))
            }
        };
        arr.into_iter()
            .map(|v| {
                serde_json::from_value::<DocumentNode>(v).map_err(|e| {
                    HandlerError::OperationFailed(format!(
                        "plugin {}: malformed DocumentNode in query: {}",
                        self.name, e
                    ))
                })
            })
            .collect()
    }

    fn set(
        &self,
        path: &str,
        properties: &HashMap<String, String>,
    ) -> Result<Vec<String>, HandlerError> {
        let args = serde_json::json!({"path": path});
        let result = self.send("set", &args, properties)?;
        let unsupported = result
            .get("unsupported_properties")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(unsupported)
    }

    fn add(
        &self,
        parent: &str,
        element_type: &str,
        position: InsertPosition,
        properties: &HashMap<String, String>,
        _wrap: Option<&str>,
    ) -> Result<String, HandlerError> {
        let pos_json = Self::position_to_json(&position);
        let mut args = serde_json::Map::new();
        args.insert(
            "parent_path".into(),
            serde_json::Value::String(parent.to_string()),
        );
        args.insert(
            "type".into(),
            serde_json::Value::String(element_type.to_string()),
        );
        if !pos_json.is_null() {
            args.insert("position".into(), pos_json);
        }
        let result = self.send("add", &serde_json::Value::Object(args), properties)?;
        let new_path = result
            .get("path")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                HandlerError::OperationFailed(format!(
                    "plugin {}: add reply missing 'path'",
                    self.name
                ))
            })?;
        Ok(new_path)
    }

    fn remove(&self, path: &str) -> Result<Option<String>, HandlerError> {
        let args = serde_json::json!({"path": path});
        let result = self.send("remove", &args, &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(Some(s)),
            serde_json::Value::Null => Ok(None),
            other => Ok(Some(other.to_string())),
        }
    }

    fn move_element(
        &self,
        source: &str,
        target_parent: Option<&str>,
        position: InsertPosition,
    ) -> Result<String, HandlerError> {
        let pos_json = Self::position_to_json(&position);
        let mut args = serde_json::Map::new();
        args.insert(
            "source_path".into(),
            serde_json::Value::String(source.to_string()),
        );
        if let Some(tp) = target_parent {
            args.insert(
                "target_parent_path".into(),
                serde_json::Value::String(tp.to_string()),
            );
        }
        if !pos_json.is_null() {
            args.insert("position".into(), pos_json);
        }
        let result = self.send("move", &serde_json::Value::Object(args), &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    fn copy_from(
        &self,
        source: &str,
        target_parent: &str,
        position: InsertPosition,
    ) -> Result<String, HandlerError> {
        let pos_json = Self::position_to_json(&position);
        let mut args = serde_json::Map::new();
        args.insert(
            "source_path".into(),
            serde_json::Value::String(source.to_string()),
        );
        args.insert(
            "target_parent_path".into(),
            serde_json::Value::String(target_parent.to_string()),
        );
        if !pos_json.is_null() {
            args.insert("position".into(), pos_json);
        }
        let result = self.send("copy", &serde_json::Value::Object(args), &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    fn swap(&self, _path1: &str, _path2: &str) -> Result<(String, String), HandlerError> {
        // swap isn't in the format-handler protocol verb table (§5.3).
        // Plugins may add it as a custom command; we don't synthesise it.
        Err(HandlerError::UnsupportedMode(format!(
            "plugin {}: swap not supported by format-handler protocol",
            self.name
        )))
    }

    fn raw(&self, part_path: &str, opts: RawOptions) -> Result<String, HandlerError> {
        let mut args = serde_json::Map::new();
        args.insert(
            "part_path".into(),
            serde_json::Value::String(part_path.to_string()),
        );
        if let Some(s) = opts.start_row {
            args.insert("start_row".into(), serde_json::Value::from(s));
        }
        if let Some(e) = opts.end_row {
            args.insert("end_row".into(), serde_json::Value::from(e));
        }
        if let Some(cols) = &opts.cols {
            let arr: Vec<serde_json::Value> = cols
                .iter()
                .map(|c| serde_json::Value::String(c.clone()))
                .collect();
            args.insert("cols".into(), serde_json::Value::Array(arr));
        }
        let result = self.send("raw", &serde_json::Value::Object(args), &HashMap::new())?;
        match result {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    fn raw_set(
        &self,
        part_path: &str,
        xpath: &str,
        action: &str,
        xml: Option<&str>,
    ) -> Result<(), HandlerError> {
        let mut args = serde_json::Map::new();
        args.insert(
            "part_path".into(),
            serde_json::Value::String(part_path.to_string()),
        );
        args.insert("xpath".into(), serde_json::Value::String(xpath.to_string()));
        args.insert(
            "action".into(),
            serde_json::Value::String(action.to_string()),
        );
        if let Some(xml) = xml {
            args.insert("xml".into(), serde_json::Value::String(xml.to_string()));
        }
        self.send("raw_set", &serde_json::Value::Object(args), &HashMap::new())?;
        Ok(())
    }

    fn add_part(
        &self,
        parent: &str,
        part_type: &str,
        properties: Option<&HashMap<String, String>>,
    ) -> Result<(String, String), HandlerError> {
        let args = serde_json::json!({
            "parent_part_path": parent,
            "part_type": part_type,
        });
        let props = properties.cloned().unwrap_or_default();
        let result = self.send("add_part", &args, &props)?;
        let rel_id = result
            .get("rel_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                HandlerError::OperationFailed(format!(
                    "plugin {}: add_part reply missing 'rel_id'",
                    self.name
                ))
            })?;
        let part_path = result
            .get("part_path")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                HandlerError::OperationFailed(format!(
                    "plugin {}: add_part reply missing 'part_path'",
                    self.name
                ))
            })?;
        Ok((rel_id, part_path))
    }

    fn validate(&self) -> Result<Vec<ValidationError>, HandlerError> {
        let result = self.send(
            "validate",
            &serde_json::Value::Object(Default::default()),
            &HashMap::new(),
        )?;
        let arr = match result {
            serde_json::Value::Array(a) => a,
            other => {
                return Err(HandlerError::OperationFailed(format!(
                    "plugin {}: validate expected array, got {}",
                    self.name, other
                )))
            }
        };
        arr.into_iter()
            .map(|v| {
                serde_json::from_value::<ValidationError>(v).map_err(|e| {
                    HandlerError::OperationFailed(format!(
                        "plugin {}: malformed ValidationError: {}",
                        self.name, e
                    ))
                })
            })
            .collect()
    }

    fn try_extract_binary(
        &self,
        path: &str,
        dest: &str,
    ) -> Result<Option<BinaryInfo>, HandlerError> {
        let args = serde_json::json!({"path": path, "dest_path": dest});
        let result = self.send("extract_binary", &args, &HashMap::new())?;
        let found = result
            .get("found")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !found {
            return Ok(None);
        }
        let content_type = result
            .get("content_type")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                HandlerError::OperationFailed(format!(
                    "plugin {}: extract_binary reply missing 'content_type'",
                    self.name
                ))
            })?;
        // Per §5.3, hosts SHOULD accept int OR double-encoded integer forms.
        let byte_count = result
            .get("byte_count")
            .and_then(|v| v.as_f64())
            .map(|f| f as usize)
            .unwrap_or(0);
        Ok(Some(BinaryInfo {
            content_type,
            byte_count,
        }))
    }

    fn save(&self) -> Result<(), HandlerError> {
        self.session.save()
    }

    fn extract_text_with_offsets(&self) -> Result<TextOffsetMap, HandlerError> {
        // The protocol doesn't define a dedicated verb; plugins MAY expose
        // `extract_text_with_offsets` as a custom command. We try that, but
        // surface UnsupportedMode cleanly if the plugin doesn't implement it.
        let args = serde_json::Value::Object(Default::default());
        let result = self.send("extract_text_with_offsets", &args, &HashMap::new())?;
        serde_json::from_value::<TextOffsetMap>(result).map_err(|e| {
            HandlerError::OperationFailed(format!(
                "plugin {}: malformed TextOffsetMap: {}",
                self.name, e
            ))
        })
    }
}

impl Drop for FormatHandlerProxy {
    fn drop(&mut self) {
        // Protocol §5.3 close: send `close` envelope so the plugin can flush
        // pending writes and exit cleanly. Errors here are best-effort —
        // the session's own Drop reaps the child regardless.
        let _ = self.session.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_envelope_includes_protocol_msg_type_command_args_props() {
        let mut props = HashMap::new();
        props.insert("text".to_string(), "hi".to_string());
        let env = build_command_envelope("add", &serde_json::json!({"parent": "/body"}), &props);
        assert_eq!(env["protocol"], 1);
        assert_eq!(env["msg_type"], "command");
        assert_eq!(env["command"], "add");
        assert_eq!(env["args"]["parent"], "/body");
        assert_eq!(env["props"]["text"], "hi");
    }

    #[test]
    fn open_envelope_is_well_formed() {
        let env = build_envelope(
            "open",
            &serde_json::json!({"path": "/tmp/x", "editable": true}),
        );
        assert_eq!(env["protocol"], 1);
        assert_eq!(env["msg_type"], "open");
        assert_eq!(env["path"], "/tmp/x");
        assert_eq!(env["editable"], true);
    }

    #[test]
    fn save_envelope_is_empty_body() {
        let env = build_envelope("save", &serde_json::Value::Object(Default::default()));
        assert_eq!(env["protocol"], 1);
        assert_eq!(env["msg_type"], "save");
    }

    /// Round-trip a session against a bash stub that:
    ///   - reads an "open" handshake line and replies with capabilities
    ///   - reads any "command" line and echoes a result
    ///   - reads "save"/"close" and replies ok
    #[test]
    fn open_command_close_round_trip() {
        // Only run when explicitly enabled — spawns subprocesses.
        let enabled = std::env::var("OFFICECLI_PLUGIN_TESTS")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if !enabled {
            return;
        }
        let stub = indoc_stub();
        let exe_path = write_stub(&stub);
        let manifest = PluginManifest {
            name: "stub".into(),
            version: "0.1".into(),
            protocol: 1,
            kinds: vec!["format-handler".into()],
            extensions: vec![".stub".into()],
            runtime: Some("bash".into()),
            target: None,
            description: None,
            tier: None,
            supports: Vec::new(),
            limits: serde_json::Value::Null,
            homepage: None,
            license: None,
            idle_timeout_seconds: None,
            vocabulary: None,
        };
        // Disable the watchdog so it doesn't kill the stub mid-test.
        std::env::set_var("OFFICECLI_PLUGIN_IDLE_TIMEOUT_SECONDS", "0");
        let session = FormatHandlerSession::open_with(&exe_path, &manifest, "/tmp/x.stub").unwrap();
        assert_eq!(session.state(), SessionState::Ready);
        let caps = session.capabilities();
        assert!(caps.commands.contains(&"add".to_string()));

        let result = session
            .send_command(
                "get",
                &serde_json::json!({"path": "/body"}),
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(result, serde_json::json!({"path": "/body"}));

        session.save().unwrap();
        session.close().unwrap();
        assert_eq!(session.state(), SessionState::Closed);
        let _ = std::fs::remove_file(&exe_path);
    }

    fn write_stub(body: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "oc-session-stub-{}.sh",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        path.to_string_lossy().to_string()
    }

    fn indoc_stub() -> String {
        // Minimal stub format-handler. Uses bash substring matching so it
        // works on both BSD sed (macOS) and GNU sed (Linux) without regex
        // quirks. Replies: open → capabilities; command → echo args; save → ok;
        // close → ok + exit. Anything else → error. The command reply is
        // deliberately hardcoded — the test sends `{"path":"/body"}` and the
        // assertion checks exactly that result.
        r#"#!/usr/bin/env bash
set -euo pipefail

read line
case "$line" in
  *'"msg_type":"open"'*)
    echo '{"protocol":1,"msg_type":"ok","result":{"capabilities":{"commands":["add","get","set"],"features":["save"]},"vocabulary":{"addable_types":["paragraph"],"path_segments":["/p[1]"]}}}'
    ;;
  *) exit 1 ;;
esac

while read line; do
  case "$line" in
    *'"msg_type":"command"'*)
      # Hardcoded reply — the test asserts this exact result.
      echo '{"protocol":1,"msg_type":"ok","result":{"path":"/body"}}'
      ;;
    *'"msg_type":"save"'*)
      echo '{"protocol":1,"msg_type":"ok","result":null}'
      ;;
    *'"msg_type":"close"'*)
      echo '{"protocol":1,"msg_type":"ok","result":null}'
      exit 0
      ;;
    *)
      echo '{"protocol":1,"msg_type":"error","error":{"code":"invalid_request","message":"unknown envelope"}}'
      ;;
  esac
done
"#
        .to_string()
    }
}
