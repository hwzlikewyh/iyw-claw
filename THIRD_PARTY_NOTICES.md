# Third-Party Notices

## agent-browser

- Project: `vercel-labs/agent-browser`
- Version: `0.35.1`
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

- Node.js `24.19.0` - https://nodejs.org/dist/v24.19.0/ - MIT License. The
  upstream archive includes its license and notice files.
- uv `0.12.1` - https://github.com/astral-sh/uv/releases/tag/0.12.1 - MIT
  License or Apache License 2.0. The upstream archive includes `LICENSE.txt`.
- Git for Windows MinGit `2.55.0.windows.3` -
  https://github.com/git-for-windows/git/releases/tag/v2.55.0.windows.3 - GNU
  General Public License v2.0. The upstream archive includes its license
  files.
- GitHub Desktop dugite-native `2.53.0-4` -
  https://github.com/desktop/dugite-native/releases/tag/v2.53.0-4 - GNU
  General Public License v2.0 and the licenses of its bundled dependencies.
- `@agentclientprotocol/codex-acp@1.4.0` -
  https://www.npmjs.com/package/@agentclientprotocol/codex-acp - Apache License
  2.0. The npm package includes its license file.
- `@openai/codex@0.147.0` and its target-specific optional package -
  https://www.npmjs.com/package/@openai/codex - Apache License 2.0. The npm
  packages include their license files.

The runtime-seed builder records the exact target, file list, byte sizes, and
SHA-256 digests in `runtime-seed/manifest.json`; the application verifies these
values before activation. License files shipped by upstream archives and npm
packages remain in their respective component directories.
