# 星河 Worker 历史源码

`iyw-xinghe-worker` is retained for historical ABI and cache references. It is
not part of the desktop build. The current desktop links the pinned Codex graph
into `iyw-claw.exe` and calls it through Rust APIs in `harness/codex`.

The old `cdylib` boundary is retired. No DLL, worker executable, C ABI entry
point, or worker-side credential environment is required for a normal release.

The active runtime receives validated paths, configuration, and the API key as
typed host-owned startup arguments. Secrets are not placed in command-line
arguments, embedded resources, or persistent Codex credentials.

Normal desktop build hooks invoke `prepare-xinghe-worker.mjs` only to write and
validate `resources/xinghe-worker/runtime.json`. The metadata records the pinned
source identity; it is not a binary payload. The Tauri resource directory is
kept for package identity and upgrade checks.

Windows sandbox setup and command-runner roles are compiled into the same EXE.
They are launched through internal flags when Windows requires a restricted or
elevated child role; this preserves OS isolation while removing helper files.

The application embeds the locked upstream identity and a runtime marker. Bundle
verification checks those bytes in the main executable and checks that the
resource directory contains metadata only.

The Windows app is built with the static MSVC CRT flag so the embedded runtime
does not require adjacent Xinghe or CRT DLL files. OS libraries and the normal
Tauri WebView2 installation remain external platform prerequisites.

The current locked source is Codex 0.156.1. Compilation and bundle checks do not
prove authenticated end-to-end behavior. Release acceptance must cover the
compatibility matrix in the harness design before these changes are published.
