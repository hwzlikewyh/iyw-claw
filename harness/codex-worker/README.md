# Codex Worker Library

`iyw-codex-worker` is a private dynamic library, not a user-facing executable.
The desktop application starts its own executable with `--internal-codex-worker`
only for an opted-in Codex connection; that child then loads this library and
serves ACP over inherited stdin/stdout.

Keeping the upstream Codex graph in this `cdylib` prevents the main iyw-claw
crate from resolving or linking its SQLite dependency graph. The C ABI exports
only no-argument entry points. No Rust values, credentials, or pointers cross
the dynamic-library boundary.

The worker reads its paths and session binding from environment variables set by
the parent process. They must never be supplied as command-line arguments or
written to diagnostics. The child resolves the library only from the desktop
application's private resource locations; it does not accept a library path
override or provide a public plugin mechanism.

For the isolated packaging experiment, run
`node src-tauri/scripts/prepare-codex-worker.mjs --target <triple>` and pass
`--config src-tauri/tauri.codex-worker.conf.json` to an explicitly experimental
Tauri build. The normal desktop scripts and release workflows do not invoke
either step.
