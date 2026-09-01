# Managed Browser Runtime Prefetch Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Download and activate the managed Chrome for Testing engine from Fusion after desktop startup, without bundling it in installers or automatically opening the user's normal browser.

**Architecture:** Keep `BrowserSessionManager` and the bundled `agent-browser` controller as the trusted control plane. Add `browser-engine` to the existing managed-tool version center, reuse its signed TOS ticket and resumable installer, and make browser runtime dependency resolution prefer the verified managed engine. A startup task schedules one silent prefetch and shares any in-flight install with the first foreground browser request.

**Tech Stack:** Rust/Tauri 2, Tokio, SeaORM/SQLite client inventory, Go/Hertz Fusion API, existing Agent Platform catalog/download tickets, TOS/CDN artifacts, SHA-256 and Minisign verification, GitHub Actions release matrix.

## Global Constraints

- Do not bundle Chrome for Testing in the desktop installer.
- Browser background prefetch must not block normal app startup or show a UI surface.
- Do not launch, attach to, or terminate a user's ordinary Chrome/Edge process during managed runtime startup.
- Reuse `GET /agent-platforms/v1/tools/{toolId}/versions`, `POST /agent-platforms/v1/tools/resolve`, and `POST /agent-platforms/v1/tools/download`.
- Verify artifact size, SHA-256, detached signature, archive layout, executable version, and a bounded launch/CDP probe before activation.
- Preserve the active and last-known-good browser engine when a new download or activation fails.
- Use one in-flight install per desktop process and one bounded retry for foreground runtime startup.
- Do not introduce automatic OpenCLI fallback.
- Preserve unrelated dirty work and do not run broad `git add`.
- Project verification defaults to static checks; do not claim desktop E2E unless it is actually run.

---

### Task 1: Extend Managed Tool Identity For Browser Engine

**Files:**
- Modify: `iyw-claw/src-tauri/src/acp/version_center/capability.rs`
- Modify: `iyw-claw/src-tauri/src/acp/version_center/installer/runtime.rs`
- Modify: `iyw-claw/src-tauri/src/acp/version_center/installer/archive.rs`
- Modify: `iyw-claw/src-tauri/src/acp/version_center/installer/manifest.rs`
- Modify: `iyw-claw/src-tauri/src/acp/version_center/installer/component.rs`
- Modify: `iyw-claw/src-tauri/src/acp/version_center/installer/tools.rs`
- Modify: `iyw-claw/src-tauri/src/acp/version_center/types.rs`
- Test: existing Rust unit tests under `iyw-claw/src-tauri/src/acp/version_center/`

**Interfaces:**
- Consumes: existing `ToolOffer`, `DownloadTicket`, `install_managed_tool`, `download_resumable`, and artifact signature verification.
- Produces: trusted tool ID `browser-engine`, platform-aware executable resolution, and a reusable install entry point that returns a verified active engine directory.

- [ ] **Step 1: Add the compiled tool identity and platform mapping**

  Extend `TOOL_IDS` with `browser-engine`. Replace the current tool-only platform path match with a browser-aware path mapping that supports `win-x64`, `darwin-x64`, `darwin-arm64`, `linux-x64`, and `linux-arm64`; retain existing `git`, `node`, and `uv` paths unchanged.

- [ ] **Step 2: Define the browser-engine archive layout and executable lookup**

  Accept only ZIP artifacts for `browser-engine`. Require a payload rooted at one directory and locate `chrome.exe` on Windows or `chrome` on Unix. Reject symlinks, path traversal, missing executable, and payloads outside the expected component root. Keep the generic managed-tool archive behavior unchanged for existing tools.

- [ ] **Step 3: Add a browser-engine health probe**

  Add a bounded executable probe that runs `<engine>/chrome(.exe) --version` with hidden Windows process creation and Unix executable permissions. Return a stable `BROWSER_ENGINE_PROBE_FAILED` detail when the process cannot start or does not report a non-empty version.

- [ ] **Step 4: Add focused validation coverage**

  Cover `browser-engine` as a known tool, reject unsupported target/arch combinations, accept each supported target path, reject non-ZIP artifacts, and reject archives without the expected executable. Run:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml version_center --lib
  ```

  Expected: existing tests and the new browser-engine tests pass.

- [ ] **Step 5: Commit only this task's files**

  ```powershell
  git add src-tauri/src/acp/version_center/capability.rs src-tauri/src/acp/version_center/installer/runtime.rs src-tauri/src/acp/version_center/installer/archive.rs src-tauri/src/acp/version_center/installer/manifest.rs src-tauri/src/acp/version_center/installer/component.rs src-tauri/src/acp/version_center/installer/tools.rs src-tauri/src/acp/version_center/types.rs
  git commit -m "feat(browser): register managed browser engine tool"
  ```

### Task 2: Publish Browser Engine Artifacts Through Fusion

**Files:**
- Modify: `iyw-fusion-api/internal/domain/agentrelease/capability.go` or the current tool allowlist source discovered in Task 1
- Modify: `iyw-fusion-api/internal/application/agentrelease/tool_queries.go`
- Modify: `iyw-fusion-api/internal/application/agentrelease/tool_download.go`
- Modify: `iyw-fusion-api/scripts/managed_component_registry.json` or the current managed-tool recipe source
- Modify: `iyw-fusion-api/scripts/managed_component_sync.py`
- Test: `iyw-fusion-api/internal/domain/agentrelease/*_test.go`
- Test: `iyw-fusion-api/scripts/test_*managed*` when an existing focused test module covers recipe construction

**Interfaces:**
- Consumes: `browser-engine` client requests with `runtime=desktop`, exact target/arch, and the existing managed mirror artifact pipeline.
- Produces: immutable signed ZIP artifacts and public resolve/download offers for the browser engine.

- [ ] **Step 1: Add browser-engine to the trusted server allowlist**

  Add `browser-engine` as a managed tool key without accepting arbitrary tool IDs. Keep delivery kind `binary`, package kind `zip`, and the existing runtime/target/arch checks.

- [ ] **Step 2: Define the six artifact coordinates and expected payloads**

  Register Windows x64, macOS x64/arm64, Linux x64/arm64, and the Windows ARM64 emulation policy as explicit coordinates. Each artifact must carry exact file name, size, SHA-256, package kind, and signature metadata. The release recipe must identify the Chrome for Testing version separately from the normalized catalog version.

- [ ] **Step 3: Reuse mirror-first acquisition and TOS publication**

  Extend the existing managed component sync to download Chrome for Testing from the upstream source, inspect the archive entry, upload an immutable TOS mirror, sign it, and link the ready artifact to the draft tool version. Do not proxy the large object through the Go API.

- [ ] **Step 4: Add server tests for selection and ticket invariants**

  Verify unknown browser coordinates, wrong package kind, empty or mismatched SHA-256, paused/withdrawn versions, and target/arch mismatch are rejected. Verify a valid browser-engine offer returns a ticket carrying the same artifact size, digest, signature, and file name.

- [ ] **Step 5: Run focused Go and sync validation**

  ```powershell
  go test ./internal/domain/agentrelease ./internal/application/agentrelease ./internal/adapter/httpserver/agentplatform
  python -m pytest scripts -q
  ```

  If the repository has no `scripts` pytest suite, run the existing managed-component command with `--help` and its dry-run/fixture validation entry point, and report the exact gap.

- [ ] **Step 6: Commit only this task's files**

  ```powershell
  git add internal/domain/agentrelease internal/application/agentrelease scripts
  git commit -m "feat(agent-platform): publish managed browser engine artifacts"
  ```

### Task 3: Silent Startup Prefetch And Foreground Join

**Files:**
- Create: `iyw-claw/src-tauri/src/browser/engine_prefetch.rs`
- Modify: `iyw-claw/src-tauri/src/browser/mod.rs`
- Modify: `iyw-claw/src-tauri/src/browser/manager.rs`
- Modify: `iyw-claw/src-tauri/src/browser/runtime_dependencies.rs`
- Modify: `iyw-claw/src-tauri/src/browser/runtime.rs`
- Modify: `iyw-claw/src-tauri/src/lib.rs` at desktop setup task spawning
- Modify: `iyw-claw/src-tauri/src/browser/types.rs`
- Modify: `iyw-claw/src-tauri/src/browser/error.rs`
- Test: focused Rust unit tests for single-flight state and dependency selection

**Interfaces:**
- Consumes: `install_managed_tool`, `AgentPlatformClient::resolve_tool/download_tool`, the active tool inventory, and the Tauri `EventEmitter`.
- Produces: `BrowserEnginePrefetch::schedule`, a foreground `ensure_engine_ready` future, and a browser runtime dependency resolver that prefers a verified managed engine.

- [ ] **Step 1: Implement single-flight prefetch state**

  Add a process-local `OnceLock`/`Mutex` or shared manager field that tracks `idle`, `running`, `ready`, and `deferred`/`failed` outcomes. `schedule` must return immediately and coalesce concurrent startup triggers. Store no URL, token, cookie, or page content.

- [ ] **Step 2: Schedule prefetch after normal desktop startup**

  Start the task only after database, account state, and normal runtime bootstrap are ready. Use `tauri::async_runtime::spawn`; do not await it from the Tauri setup callback. The task must avoid opening browser windows/tabs and must emit only internal structured progress/events.

- [ ] **Step 3: Join the same task from foreground browser start**

  Change `BrowserSessionManager::start_browser_runtime` and `ensure_runtime_running` to await the existing prefetch future when one is active. If no prefetch is active, start one foreground attempt. Never create a second browser-engine download task.

- [ ] **Step 4: Prefer the managed engine and preserve LKG**

  Update `runtime_dependencies.rs` so a verified active managed `browser-engine` is used first. If it is invalid, use the verified last-known-good version. Only if neither exists should the foreground call return a concrete engine-unavailable error. Do not automatically select or attach to system Chrome.

- [ ] **Step 5: Make errors stable and silent in background**

  Add internal error categories for offer unavailable, download failure, disk space, integrity/signature failure, archive invalid, and probe failure. Background failures log structured data and leave UI state unchanged; foreground failures map to the existing browser error envelope with retryability and no raw URL/path leakage.

- [ ] **Step 6: Add focused tests and run static validation**

  Cover coalesced scheduling, foreground join, background failure suppression, active/LKG selection, and no automatic OpenCLI path. Run:

  ```powershell
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  cargo check --manifest-path src-tauri/Cargo.toml --features tauri-runtime
  git diff --check
  ```

- [ ] **Step 7: Commit only this task's files**

  ```powershell
  git add src-tauri/src/browser src-tauri/src/lib.rs
  git commit -m "feat(browser): prefetch managed engine after startup"
  ```

### Task 4: Cross-Platform Sidecars, Release Gates, And Documentation

**Files:**
- Modify: `iyw-claw/src-tauri/scripts/prepare-agent-browser-sidecar.mjs`
- Modify: `iyw-claw/src-tauri/scripts/verify-agent-browser-bundle.mjs`
- Modify: `iyw-claw/src-tauri/src/browser/sidecar.rs`
- Modify: `iyw-claw/src-tauri/src/browser/engine.rs`
- Modify: `iyw-claw/src-tauri/src/browser/windows_process.rs` only where platform-neutral launch abstractions are required
- Modify: `iyw-claw/.github/workflows/release-tauri.yml`
- Modify: `iyw-claw/.github/workflows/release-self-hosted-test.yml`
- Modify: `iyw-claw/docs/browser-agent-usage.md`
- Modify: `iyw-claw/docs/superpowers/specs/2026-09-01-managed-browser-runtime-prefetch-design.md` only if implementation evidence changes an assumption

**Interfaces:**
- Consumes: upstream `agent-browser` v0.35.2 platform assets and the browser-engine Fusion offers from Tasks 1-3.
- Produces: signed platform sidecars, release verification gates, and user/agent documentation that does not suggest automatic OpenCLI takeover.

- [ ] **Step 1: Stage target-specific sidecars**

  Replace the Windows-only staging constants with the upstream release asset map for Windows x64, macOS x64/arm64, and Linux x64/arm64. Pin exact version, size, SHA-256, and expected executable name. Keep Windows x86 excluded explicitly because no upstream sidecar exists.

- [ ] **Step 2: Verify installed sidecars on every supported target**

  Extend staged and installed bundle verification to reject missing, zero-byte, wrong-architecture, wrong-size, wrong-digest, and wrong-version sidecars. Keep macOS signing/notarization and Windows Authenticode checks separate from the sidecar digest check.

- [ ] **Step 3: Add platform-aware engine discovery**

  Use only the managed engine path for execution. Keep system-browser discovery solely as an optional stopped profile seed source, with platform-specific paths for macOS and Linux. Do not make a system browser the fallback execution engine.

- [ ] **Step 4: Add release and installed-client gates**

  Add CI checks for sidecar stage/install and a prepared browser-engine fixture that executes `start -> about:blank -> CDP -> snapshot -> stream frame -> close` on each required target. Add interrupted-download and digest-failure rollback fixtures where the runner supports them.

- [ ] **Step 5: Update usage and support documentation**

  Document that the engine is downloaded silently after startup, where to find redacted diagnostics, how to trigger a foreground retry, and that OpenCLI/ordinary Chrome is never selected automatically. Do not document secrets, signed URLs, local profile paths, or raw object keys.

- [ ] **Step 6: Run final static checks**

  ```powershell
  node src-tauri/scripts/verify-sidecar-bundle.mjs --target x86_64-pc-windows-msvc
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  cargo check --manifest-path src-tauri/Cargo.toml --features tauri-runtime
  git diff --check
  ```

  Native macOS/Linux installed-client checks are not claimed unless their runners execute successfully.

- [ ] **Step 7: Commit only this task's files**

  ```powershell
  git add src-tauri/scripts/prepare-agent-browser-sidecar.mjs src-tauri/scripts/verify-agent-browser-bundle.mjs src-tauri/src/browser/sidecar.rs src-tauri/src/browser/engine.rs .github/workflows/release-tauri.yml .github/workflows/release-self-hosted-test.yml docs/browser-agent-usage.md
  git commit -m "feat(browser): gate cross-platform managed runtime delivery"
  ```

---

## Final Review

- [ ] Review all changed files for accidental modification of unrelated dirty work.
- [ ] Verify no code path invokes OpenCLI or ordinary Chrome automatically.
- [ ] Verify all downloads use Fusion tickets and direct TOS/CDN transfer.
- [ ] Verify only validated artifacts are delivered to the conversation.
- [ ] Report static checks, test results, and any unverified native-platform gaps accurately.
