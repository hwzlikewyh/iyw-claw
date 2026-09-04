# Third-Party Notices

## agent-browser

- Project: `vercel-labs/agent-browser`
- Version: `0.36.0`
- Source: https://github.com/vercel-labs/agent-browser
- License: Apache License 2.0

The Windows x64 desktop package includes the unmodified upstream
`agent-browser` executable. Its exact size and SHA-256 digest are verified at
build time and before every launch. The Apache License 2.0 text is included in
this distribution as `LICENSE`.

## Bundled runtime seed

The desktop packages for Windows x64, macOS x64/arm64, and Linux x64/arm64
contain target-specific runtime components. Windows x86 intentionally does not
contain this seed and keeps the online Version Center installation path.

- Node.js `24.20.0` - https://nodejs.org/dist/v24.20.0/ - MIT License. The
  upstream archive includes its license and notice files.
- uv `0.12.9` - https://github.com/astral-sh/uv/releases/tag/0.12.9 - MIT
  License or Apache License 2.0. The upstream archive includes `LICENSE.txt`.
- Git for Windows MinGit `2.55.0.windows.5` -
  https://github.com/git-for-windows/git/releases/tag/v2.55.0.windows.5 - GNU
  General Public License v2.0. The upstream archive includes its license
  files.
- GitHub Desktop dugite-native `2.53.0-4` -
  https://github.com/desktop/dugite-native/releases/tag/v2.53.0-4 - GNU
  General Public License v2.0 and the licenses of its bundled dependencies.
- `@agentclientprotocol/codex-acp@1.8.0` -
  https://www.npmjs.com/package/@agentclientprotocol/codex-acp - Apache License
  2.0. The npm package includes its license file.
- `@openai/codex@0.152.1` and its target-specific optional package -
  https://www.npmjs.com/package/@openai/codex - Apache License 2.0. The npm
  packages include their license files.

The runtime-seed builder records the exact target, file list, byte sizes, and
SHA-256 digests in `runtime-seed/manifest.json`; the application verifies these
values before activation. License files shipped by upstream archives and npm
packages remain in their respective component directories.

## Optional Codex in-process harness

- Project: `openai/codex` (`codex-rs` App Server components)
- Locked source: `rust-v0.152.1` / `5adb68a49933ae446bf11935662c83dba55a0804`
- Source: https://github.com/openai/codex
- License: Apache License 2.0

The optional `harness/codex` integration locks the upstream source revision and
is not part of the default application runtime. Its repository-local
`patches/codex-utils-pty` is a copy of upstream `codex-rs/utils/pty` with only
the documented Windows pointer-cast compatibility adjustments. It remains
subject to the same Apache License 2.0; the source patch is re-evaluated on
every upstream update.

The experimental `harness/codex-worker` package links the same locked source as
a private platform dynamic library (`iyw_codex_worker.dll`,
`libiyw_codex_worker.dylib`, or `libiyw_codex_worker.so`). It is loaded by the
single `iyw-claw` executable after a self-reexec and is not a second
user-facing executable. The library is staged only by
`src-tauri/scripts/prepare-codex-worker.mjs` and is not included by the normal
release workflows.
