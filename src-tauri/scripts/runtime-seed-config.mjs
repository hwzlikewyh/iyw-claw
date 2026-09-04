const TARGETS = {
  "x86_64-pc-windows-msvc": {
    os: "windows",
    arch: "x86_64",
    platform: "win-x64",
    npm: ["win32", "x64"],
  },
  "i686-pc-windows-msvc": {
    os: "windows",
    arch: "x86",
    platform: "win-x86",
    npm: ["win32", "ia32"],
    skipped: true,
  },
  "x86_64-apple-darwin": {
    os: "macos",
    arch: "x86_64",
    platform: "darwin-x64",
    npm: ["darwin", "x64"],
  },
  "aarch64-apple-darwin": {
    os: "macos",
    arch: "aarch64",
    platform: "darwin-arm64",
    npm: ["darwin", "arm64"],
  },
  "x86_64-unknown-linux-gnu": {
    os: "linux",
    arch: "x86_64",
    platform: "linux-x64",
    npm: ["linux", "x64"],
  },
  "aarch64-unknown-linux-gnu": {
    os: "linux",
    arch: "aarch64",
    platform: "linux-arm64",
    npm: ["linux", "arm64"],
  },
}

const PINNED_NODE_VERSION = "24.20.0"

const DOWNLOADS = {
  node: {
    version: PINNED_NODE_VERSION,
    base: `https://nodejs.org/dist/v${PINNED_NODE_VERSION}/`,
    "win-x64": [
      `node-v${PINNED_NODE_VERSION}-win-x64.zip`,
      "6cac9ffbca8f6a47091e4b5c772e0606049c3871cb67d900c0cedde630e545ba",
    ],
    "darwin-x64": [
      `node-v${PINNED_NODE_VERSION}-darwin-x64.tar.gz`,
      "9e5b2644cf107befb6aefca676b96d3296bc10138096f022ed378d6233ed81f4",
    ],
    "darwin-arm64": [
      `node-v${PINNED_NODE_VERSION}-darwin-arm64.tar.gz`,
      "40e5607e5ecb3db9192723776da2d75d966260fc74a7a9e731c1bd67dda96bc8",
    ],
    "linux-x64": [
      `node-v${PINNED_NODE_VERSION}-linux-x64.tar.gz`,
      "855d581f8a4eb1a8117e3426de25fe02770592febcfb31369aee1ffbfee9e8ec",
    ],
    "linux-arm64": [
      `node-v${PINNED_NODE_VERSION}-linux-arm64.tar.gz`,
      "3515603e2487879a39bc75716f1a2affd027500c64ba50e845cf72cb33219013",
    ],
  },
  uv: {
    version: "0.12.9",
    base: "https://github.com/astral-sh/uv/releases/download/0.12.9/",
    "win-x64": [
      "uv-x86_64-pc-windows-msvc.zip",
      "ddbfcee1ac615a0499f6aa97b5ec8ebdf3ee4a7714a48055ec2ba0030e3cf810",
    ],
    "darwin-x64": [
      "uv-x86_64-apple-darwin.tar.gz",
      "e1ca175824f1056589ce9908f7631879ebc3c36535b5e63dc06510beb370b4c1",
    ],
    "darwin-arm64": [
      "uv-aarch64-apple-darwin.tar.gz",
      "301f72afaf54060f92da7016cb0115bd077f43a9c8e39c1d8170a0bac80fd398",
    ],
    "linux-x64": [
      "uv-x86_64-unknown-linux-gnu.tar.gz",
      "ec7a99cd05e0cd7f80243f135ce1361c76835cb0ee60055d14d20eba8eba1460",
    ],
    "linux-arm64": [
      "uv-aarch64-unknown-linux-gnu.tar.gz",
      "c36fe17937ff6bd16dc42fc13854b5465999fcab2efe0af559381e945e3c6001",
    ],
  },
  git: {
    version: "2.55.0+windows.5",
    nonWindowsVersion: "2.53.0-4",
    base: "https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.5/",
    nonWindowsBase:
      "https://github.com/desktop/dugite-native/releases/download/v2.53.0-4/",
    "win-x64": [
      "MinGit-2.55.0.5-64-bit.zip",
      "56d7b226b7693196cfc71fef26568f536c4a021ab6c37ff2db4287bed908e96e",
    ],
    "darwin-x64": [
      "dugite-native-v2.53.0-4098283-macOS-x64.tar.gz",
      "ae6686718aa34f4140424db16b92a47dcffd6d1f312eb8b5f3b267f7404e2680",
    ],
    "darwin-arm64": [
      "dugite-native-v2.53.0-4098283-macOS-arm64.tar.gz",
      "f9dc64635a5b62fbd7ad95db73268bbb8912255ac516d65d37bf7af22fcb8ffe",
    ],
    "linux-x64": [
      "dugite-native-v2.53.0-4098283-ubuntu-x64.tar.gz",
      "cca76aa31ad9e835e771ee7f55b73934777fbd8d16757a10d307ba06de860901",
    ],
    "linux-arm64": [
      "dugite-native-v2.53.0-4098283-ubuntu-arm64.tar.gz",
      "a161f45af4626bb7e0c688854bd4a9aee47cc514bca404cff0a5e3536ef1c0af",
    ],
  },
  "codex-acp": {
    version: "1.8.0",
    package: "@agentclientprotocol/codex-acp",
    codexPackage: "@openai/codex",
    codexVersion: "0.152.1",
  },
}

function targetInfo(target) {
  const info = TARGETS[target]
  if (!info) throw new Error(`unsupported runtime seed target: ${target}`)
  return { target, ...info }
}

function parseTarget(argv = process.argv) {
  const index = argv.indexOf("--target")
  return index >= 0 ? argv[index + 1] : process.env.TARGET
}

export { DOWNLOADS, PINNED_NODE_VERSION, TARGETS, parseTarget, targetInfo }
