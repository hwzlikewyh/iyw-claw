# Codex Worker Library

`iyw-codex-worker` is a private dynamic library, not a user-facing executable.
The desktop application starts its own executable with `--internal-codex-worker`
for a 星河 connection; that child loads this library and serves ACP over
inherited stdin/stdout. A missing or mismatched library requires an application
repair and does not silently select the external runtime.

Keeping the upstream Codex graph in this `cdylib` prevents the main iyw-claw
crate from resolving or linking its SQLite dependency graph. The C ABI exports
only no-argument entry points. No Rust values, credentials, or pointers cross
the dynamic-library boundary.

The worker reads its paths and session binding from environment variables set by
the parent process. They must never be supplied as command-line arguments or
written to diagnostics. The child resolves the library only from the desktop
application's private resource locations; it does not accept a library path
override or provide a public plugin mechanism.

Normal desktop build hooks and release workflows invoke
`node src-tauri/scripts/prepare-codex-worker.mjs --target <triple>`. The primary
Tauri resource configuration installs the resulting `resources/codex-worker`
directory as `codex-resources`. NSIS runtime seeds contain the shared Node, Git and uv tools; the
former npm 星河 seed is no longer generated or activated on desktop.

Windows also builds the pinned sandbox setup and command-runner executables.
All platforms include a private `iyw-codex-helper` executable that statically
links the existing helper dispatch. Upstream passes only allowlisted environment
variables to filesystem helpers, and Windows also copies helpers into the sandbox.
A copied desktop executable could not locate its worker DLL or active-worker
marker. The helper accepts only existing internal helper modes and does not
start a second agent or public CLI.

The worker exports its ABI and core version as integers and embeds its locked
upstream identity. Preparation verifies the binary architecture and identity;
the Windows verifier also reads the export table and checks each helper's
architecture. Installed helpers must match the staged bytes. The application validates
the identity before launch and the ABI before invoking either entry point.

The implementation branch has passed Windows worker `cargo check`. That does
not prove desktop compilation, signed package contents, or authenticated
end-to-end behavior. Release acceptance must cover the compatibility matrix
in the harness design before these changes are published.
