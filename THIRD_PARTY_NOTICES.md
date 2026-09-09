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

The runtime-seed builder records the exact target, file list, byte sizes, and
SHA-256 digests in `runtime-seed/manifest.json`; the application verifies these
values before activation. License files shipped by upstream archives and npm
packages remain in their respective component directories.

## 内置星河运行时

- Project: `openai/codex` (`codex-rs` App Server components)
- Locked source: `rust-v0.153.4` / `3d2ee51ca2d5db578f328aa75e20aa22c0197c9a`
- Source: https://github.com/openai/codex
- License: Apache License 2.0

The `harness/codex` integration locks the upstream source revision. Its local
patches for `codex-utils-pty`, `codex-shell-command`, `codex-git-utils` and
`codex-windows-sandbox` retain the Apache License 2.0 and make the documented
Windows pointer, hidden-window and package-name compatibility adjustments.
The patches are re-evaluated on every upstream update.

The `harness/xinghe-worker` package links the same locked source as
a private platform dynamic library (`iyw_xinghe_worker.dll`,
`libiyw_xinghe_worker.dylib`, or `libiyw_xinghe_worker.so`). It is loaded by the
single `iyw-claw` executable after a self-reexec and is not a second
user-facing executable. Desktop build and release workflows run
`src-tauri/scripts/prepare-xinghe-worker.mjs` to stage the library under
`xinghe-resources`, with `iyw-xinghe-helper` and the Windows-only
`xinghe-command-runner.exe` and `xinghe-windows-sandbox-setup.exe` helpers.
The runtime seed no longer contains the former npm agent packages.

## Microsoft Visual C++ Runtime

Windows packages include the unmodified Microsoft `vcruntime140.dll` and,
when required by the selected target, `vcruntime140_1.dll`. These files come
from the matching architecture's Visual Studio release redistributable directory
and are distributed under the applicable Microsoft Visual Studio license terms.
They are installed beside the built-in worker and copied with sandbox helpers.
Redistribution reference:
https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files
