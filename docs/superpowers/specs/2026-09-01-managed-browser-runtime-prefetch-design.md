# Managed Browser Runtime Prefetch Design

## Status

- Design date: 2026-09-01
- User decision: do not add Chrome for Testing to the desktop installer.
- User decision: prefetch the browser engine silently after the first app startup.
- User decision: support Windows, macOS, and Linux desktop targets.
- Implementation is in progress. The first implementation slice wires the existing
  managed-tool installer, startup prefetch coordinator, and cross-platform sidecar
  staging; native installed-client smoke remains a release-gate task.

## Objective

Make the iyw-claw managed browser available on supported desktop machines without depending on a user's running Chrome, increasing every installer by about 200 MiB, or silently switching to OpenCLI.

The implementation is successful when:

1. A fresh desktop installation starts normally while browser-engine acquisition runs independently in the background.
2. A verified local browser engine starts without network access and survives application updates, Windows updates, and abnormal previous shutdowns.
3. Missing or damaged engine files are downloaded through Fusion-managed artifacts, verified before activation, and never replace a working local version on failure.
4. Background acquisition never opens a browser window, an operating-system dialog, or a user Chrome process. Failure remains internal until the user asks to use the managed browser.
5. Windows x64, macOS x64/ARM64, and Linux x64/ARM64 release jobs prove the installed sidecar and browser runtime through a real launch/CDP/snapshot/close smoke test before publication.

## Scope

### Included

- Native `agent-browser` sidecars for supported Tauri desktop targets.
- A Fusion-managed `browser-engine` tool containing Chrome for Testing archives.
- Silent, non-blocking startup prefetch after account and network readiness.
- Resumable downloads, integrity verification, safe extraction, activation, last-known-good fallback, and bounded cleanup.
- Cross-platform engine discovery and launch using iyw-claw-owned profiles.
- Stable diagnostics for acquisition, verification, launch, recovery, and cleanup.
- Release and installed-package smoke coverage for the supported target matrix.
- OpenCLI fallback becoming explicit opt-in rather than an automatic Agent decision.

### Excluded

- Bundling Chrome for Testing in the desktop installer.
- Replacing `BrowserSessionManager`, the built-in MCP surface, fixed-tab identity, frame streaming, or user takeover.
- Using Tauri WebView, CEF, Playwright, Steel, Browserless, or Lightpanda as the primary browser runtime.
- Server/headless runtime support in this phase.
- Automatic system package installation, `sudo`, sandbox disabling, or broad process termination.
- Reusing a live user's Chrome profile or attaching to a running user browser.

## Architecture Decision

Keep the current host-owned browser architecture and separate the controller from the engine:

```text
Tauri UI / ACP / built-in MCP
          |
BrowserSessionManager
          |
bundled native agent-browser sidecar
          |
downloaded Chrome for Testing + iyw-claw profile + private CDP
```

The sidecar remains bundled because it is small, platform-specific, and part of the trusted application control plane. The browser engine is large and changes independently, so Fusion distributes it as an application-managed component.

The sidecar must not run its own `install` or `upgrade` command. iyw-claw owns the download URL, artifact verification, extraction limits, active pointer, rollback, and cleanup.

## Platform Matrix

| Desktop target | Sidecar source | Browser engine source |
| --- | --- | --- |
| Windows x64 | `agent-browser-win32-x64.exe` | Chrome for Testing `win64` |
| macOS x64 | `agent-browser-darwin-x64` | Chrome for Testing `mac-x64` |
| macOS ARM64 | `agent-browser-darwin-arm64` | Chrome for Testing `mac-arm64` |
| Linux x64 glibc | `agent-browser-linux-x64` | Chrome for Testing `linux64` |
| Linux ARM64 glibc | `agent-browser-linux-arm64` | a pinned Linux ARM64 Chrome for Testing artifact mirrored by Fusion |

Windows ARM64 may run the released Windows x64 application, sidecar, and engine through operating-system emulation, but it is not declared a native target until iyw-claw publishes and verifies an ARM64 desktop bundle. The existing Windows x86 application remains supported without a managed browser because the upstream sidecar has no 32-bit build. Linux musl sidecars may remain a later packaging target. The current Tauri desktop release uses glibc-based distributions and is the required Linux boundary for this phase.

Each sidecar asset is pinned by version, size, and SHA-256 in the repository. macOS sidecars are signed as nested application code and included in notarization. Windows sidecars follow the application's Authenticode policy. Linux sidecars retain executable mode and are verified after installation.

## Fusion Distribution

Reuse the existing Agent Platform managed-tool contract instead of introducing a large-file proxy or a new schema.

- Add compiled tool identity `browser-engine` to the trusted server and client allowlists.
- Publish one immutable ZIP artifact per supported `runtime=desktop`, `target`, and `arch`.
- Normalize the four-part Chrome version into stable SemVer, for example `152.0.7977+64`, while retaining the exact upstream version and source asset in mirror provenance.
- Reuse existing APIs:
  - `GET /agent-platforms/v1/tools/browser-engine/versions`
  - `POST /agent-platforms/v1/tools/resolve`
  - `POST /agent-platforms/v1/tools/download`
- The download endpoint returns a short-lived TOS/CDN URL. Fusion never streams the archive body through the Go service.
- The ticket includes file name, content type, size, SHA-256, detached signature, and expiry.
- The existing toolchain signing key signs the immutable mirrored artifact. The client verifies both SHA-256 and Minisign before extraction.

No database schema change is required. Existing tool-version, distribution, artifact, policy, event, and audit tables are reused.

## Startup Prefetch

Prefetch starts only after the following conditions are true:

- The main window and normal desktop startup are ready.
- The application is not shutting down.
- A persisted account token is available for the authenticated Agent Platform endpoints.
- No verified active `browser-engine` version already satisfies the current target and policy.
- No browser-engine install task is already running.

The startup path schedules the task and returns immediately. It does not wait for resolve, download, extraction, or probing. It does not open the browser panel, create a browser tab, show a toast, or display a modal.

If the user is signed out, offline, or Fusion is unavailable, prefetch records a bounded internal outcome and stops. A later account-ready transition or foreground browser request may schedule one new attempt. The process does not poll continuously.

If a browser engine is already ready, startup performs only a cheap local marker and executable check. No download progress or detection state is shown.

## Local Storage And Activation

Use the application data root, not a user browser directory:

```text
<app-data>/browser/
  engine/
    current.json
    versions/<version>/
      chrome executable and support files
      ownership.json
    staging/<operation-id>/
    downloads/<artifact-id>.part
  profile-v1/
  runtime-<runtime-id>/
```

`ownership.json` records the managed component identity, target, architecture, version, artifact ID, size, SHA-256, and signature identity. It contains no signed URL, token, object key, cookie, or profile data.

Activation sequence:

1. Resolve an offer and request a fresh download ticket.
2. Check free disk space against archive size plus the extraction bound.
3. Resume a matching `.part` file with HTTP Range. If the server returns a full body, truncate and restart safely.
4. Renew the ticket after an authorization expiry; do not discard verified partial bytes solely because the URL expired.
5. Verify final size, SHA-256, and detached signature.
6. Extract into a unique staging directory with entry-count, total-size, path-traversal, symlink, and executable-layout checks.
7. Probe the staged executable version and perform a bounded isolated launch/CDP/close health check.
8. Rename staging to the immutable version directory and atomically replace `current.json`.
9. Retain the previous verified version as last-known-good; garbage-collect only older inactive versions.

A failed operation removes only its owned staging files. It never deletes the active version or a profile, and it never kills a process without matching the recorded executable, arguments, PID, and start time.

## Runtime Selection And Recovery

`BrowserRuntime::prepare_dependencies` resolves the engine in this order:

1. Verified active managed `browser-engine` from `current.json`.
2. Verified last-known-good managed version when the active pointer or probe is invalid.
3. `BrowserEngineNotFound` with acquisition state; system Chrome is not selected automatically.

An installed system browser may be inspected only as an optional, stopped profile seed source. It is never the managed execution binary and is never launched by this flow.

Runtime launch keeps the existing generation and shutdown gates. Startup is divided into separately timed phases:

```text
dependency_verify
profile_prepare
daemon_spawn
daemon_identity
cdp_connect
initial_tab
stream_probe
```

One failed launch may perform one owned cleanup and one fresh-runtime retry. No layer may extend this into repeated browser opens. A wedged sidecar command is bounded by the host timeout; timeout cleanup includes the exact client process, daemon, and engine tree owned by that runtime.

## User Experience

Background prefetch is silent:

- No browser window, toast, modal, notification, tray badge, or automatic panel opening.
- No visible progress when the user has not asked to use the browser.
- Internal structured events and diagnostics remain available for support.

When the user opens the managed browser:

- Ready engine: open normally.
- Prefetch in progress: the browser panel shows its existing stable-size preparation state and live download/verification progress without creating a second task.
- Deferred or failed prefetch: the foreground request performs one explicit acquisition attempt and shows the specific stable error category if it cannot proceed.
- Existing last-known-good engine: start it immediately and refresh a newer optional version in the background.

Agent tools never switch to OpenCLI automatically. External-browser fallback requires an explicit user preference or a direct instruction in the current task.

## Error Model

Add stable internal categories while preserving the public browser error envelope:

- `BROWSER_ENGINE_OFFER_UNAVAILABLE`
- `BROWSER_ENGINE_AUTH_REQUIRED`
- `BROWSER_ENGINE_DOWNLOAD_FAILED`
- `BROWSER_ENGINE_DOWNLOAD_TOO_LARGE`
- `BROWSER_ENGINE_DISK_FULL`
- `BROWSER_ENGINE_INTEGRITY_FAILED`
- `BROWSER_ENGINE_SIGNATURE_FAILED`
- `BROWSER_ENGINE_ARCHIVE_INVALID`
- `BROWSER_ENGINE_DEPENDENCY_MISSING`
- `BROWSER_ENGINE_PROBE_FAILED`
- `BROWSER_ENGINE_ACTIVATION_FAILED`
- `BROWSER_SIDECAR_UNTRUSTED`
- `BROWSER_RUNTIME_START_TIMEOUT`

Logs include installation ID hash, target, architecture, component version, artifact ID, phase, duration, byte counts, retry number, and stable error code. Logs exclude tokens, signed URLs, object keys, local profile paths, page URLs, cookies, and command payloads.

## Cross-Platform Details

### Windows

- Preserve the existing standard-user launch path when the Tauri process is elevated.
- Hide controller processes and prevent inherited stdout/stderr handles from keeping captured commands alive.
- Use exact process identity and job/tree cleanup for owned processes.
- Do not terminate ordinary `chrome.exe` processes.
- Treat Windows update shutdown as normal teardown; the next launch reclaims stale iyw-claw locks and runtime directories.

### macOS

- Bundle the matching sidecar architecture.
- Sign the sidecar and app bundle with Developer ID and notarize the final artifact.
- Store downloaded Chrome outside the signed app bundle.
- Validate architecture and executable permission after extraction.
- Never copy Chrome Safe Storage credentials or a live profile. Profile seeding remains best-effort and may be disabled when Keychain compatibility cannot be proven.

### Linux

- Support the release distributions covered by the Tauri build and browser artifact matrix.
- Diagnose missing shared libraries before launch where possible.
- Preserve Chromium sandboxing. Never add `--no-sandbox` as a generic repair.
- A visible user handoff requires a real desktop session. Xvfb is not used to pretend that a server session is user-visible.
- Missing `DISPLAY`/Wayland is a stable foreground limitation, not a reason to switch to an external browser.

## Release Pipeline

The iyw-claw release workflow must:

1. Stage the target-specific native sidecar for Windows, macOS, and Linux instead of skipping non-Windows sidecars.
2. Verify pinned version, exact size, SHA-256, and executable behavior before Tauri build.
3. Verify the sidecar inside the installed/bundled application.
4. Sign platform artifacts according to the platform policy.
5. Run an installed-client smoke with a prepared browser engine fixture:
   `start -> about:blank -> CDP -> snapshot -> stream frame -> close`.
6. Run a cold acquisition smoke against a test Fusion/TOS artifact:
   `resolve -> ticket -> download -> verify -> extract -> probe -> activate`.
7. Run an interrupted-download resume smoke and an integrity-failure rollback smoke.

Source build success does not prove installed-client behavior. A target is not declared supported until its native installed-package smoke passes.

## Validation Matrix

### Focused client checks

- Known `browser-engine` tool offer validation by runtime, target, arch, package kind, size, SHA-256, and signature.
- Resume behavior for HTTP 206, full-body 200 fallback, expired ticket, short body, oversized body, and changed artifact identity.
- Safe archive extraction and executable-layout validation.
- Atomic activation, last-known-good selection, and cleanup ownership.
- Single-flight startup prefetch and foreground join behavior.
- Automatic OpenCLI fallback remains disabled.

### Fusion checks

- `browser-engine` is accepted only for the compiled platform matrix.
- Draft publication requires every required artifact to be ready and signed.
- Resolve selects the exact desktop target and architecture.
- Download tickets preserve size, SHA-256, signature, and immutable artifact identity.
- Paused, withdrawn, blocked, mismatched, or stale-revision artifacts cannot receive a ticket.

### Installed desktop scenarios

- Fresh install with no system Chrome.
- Existing Chrome running with a locked user profile.
- Offline startup with a ready managed engine.
- Offline first startup with no engine.
- Interrupted download and application restart.
- Fusion/TOS timeout or expired URL.
- Insufficient disk space.
- Corrupt archive or digest mismatch.
- Sidecar quarantined or missing.
- Windows update or forced process termination during browser use.
- macOS Gatekeeper/notarization verification.
- Linux missing shared library, missing display, and supported desktop launch.

## Rollout

1. Publish test-only `browser-engine` artifacts for the supported matrix without changing production policy.
2. Add client support and verify source plus installed packages.
3. Enable silent prefetch for an internal installation cohort through the existing
   managed-tool policy and release controls.
4. Confirm acquisition success, bytes, duration, activation, launch, and cleanup metrics.
5. Expand rollout while preserving last-known-good behavior.
6. Enable the feature by default only after every required installed target passes its smoke gate.

The existing Windows browser remains usable until the managed-engine rollout is complete. Production policy changes, database changes, and remote publication require separate explicit authorization.
