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

const PINNED_NODE_VERSION = "24.19.0"

const DOWNLOADS = {
  node: {
    version: PINNED_NODE_VERSION,
    base: `https://nodejs.org/dist/v${PINNED_NODE_VERSION}/`,
    "win-x64": [
      `node-v${PINNED_NODE_VERSION}-win-x64.zip`,
      "57f71ab3652e797d84acddc79c81cc9ff1c6ddb2a1974cdb83f00fee9bff4c73",
    ],
    "darwin-x64": [
      `node-v${PINNED_NODE_VERSION}-darwin-x64.tar.gz`,
      "d1b5e999db158c62fe8f7267a4476b035d8bd93b1a605bac24a3f0dd166e3316",
    ],
    "darwin-arm64": [
      `node-v${PINNED_NODE_VERSION}-darwin-arm64.tar.gz`,
      "8294b7aa9b03997481c06babf1e8b270c859358f27da57a11509afe537ac381d",
    ],
    "linux-x64": [
      `node-v${PINNED_NODE_VERSION}-linux-x64.tar.gz`,
      "f625d97cd707df4ff96254916fbc5ff014f09c09effe5a1e0ca8f6d41a8789d4",
    ],
    "linux-arm64": [
      `node-v${PINNED_NODE_VERSION}-linux-arm64.tar.gz`,
      "d28c8a5bf0a808f0ed434a1dce8c54ae98f0371c0bd86ac58abc613f73e6643f",
    ],
  },
  uv: {
    version: "0.12.1",
    base: "https://github.com/astral-sh/uv/releases/download/0.12.1/",
    "win-x64": [
      "uv-x86_64-pc-windows-msvc.zip",
      "8fcb0cb46e1229065e344758980924e569bef5882ef45f46fada8fb24e06b74a",
    ],
    "darwin-x64": [
      "uv-x86_64-apple-darwin.tar.gz",
      "69d9f9a00337f25a50dcb13882052da08b8469bac11091c98c5694c3c6721467",
    ],
    "darwin-arm64": [
      "uv-aarch64-apple-darwin.tar.gz",
      "77d2906988e8074fd43f2f329ec452ebbf9b0c257ba1c66451c71de70a6baf42",
    ],
    "linux-x64": [
      "uv-x86_64-unknown-linux-gnu.tar.gz",
      "90b2f223fb69d19db49e117da601f64978593417988530aa733d456141b4bcbb",
    ],
    "linux-arm64": [
      "uv-aarch64-unknown-linux-gnu.tar.gz",
      "769d373e146692c639b5fbaae33b331c297a32e03d30448772051902df52bbf4",
    ],
  },
  git: {
    version: "2.55.0+windows.3",
    nonWindowsVersion: "2.53.0-4",
    base: "https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.3/",
    nonWindowsBase:
      "https://github.com/desktop/dugite-native/releases/download/v2.53.0-4/",
    "win-x64": [
      "MinGit-2.55.0.3-64-bit.zip",
      "f48e2d2dc74a24454adc6d8fd0ac25bf9c2386f19cfb06202b9465aaad4f9f05",
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
    version: "1.4.0",
    package: "@agentclientprotocol/codex-acp",
    codexPackage: "@openai/codex",
    codexVersion: "0.147.0",
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
