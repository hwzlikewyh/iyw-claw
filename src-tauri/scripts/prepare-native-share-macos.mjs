import { execFileSync } from "node:child_process"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"

function run(tool, args) {
  execFileSync(tool, args, { stdio: "inherit" })
}

function entitlements(values) {
  return `<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>${values}</dict></plist>`
}

function compileExtension({ target, root, bundle }) {
  const contents = join(bundle, "Contents")
  mkdirSync(join(contents, "MacOS"), { recursive: true })
  const arch = target.startsWith("aarch64-") ? "arm64" : "x86_64"
  run("xcrun", [
    "swiftc",
    "-swift-version",
    "5",
    "-O",
    "-application-extension",
    "-target",
    `${arch}-apple-macos14.0`,
    "-parse-as-library",
    "-emit-executable",
    join(root, "macos/ShareViewController.swift"),
    "-framework",
    "AppKit",
    "-Xlinker",
    "-e",
    "-Xlinker",
    "_NSExtensionMain",
    "-Xlinker",
    "-application_extension",
    "-o",
    join(contents, "MacOS/IywAssistantShare"),
  ])
}

function writeMetadata({ root, bundle, version, group }) {
  const contents = join(bundle, "Contents")
  const info = readFileSync(join(root, "macos/Info.plist"), "utf8")
    .replaceAll("{{version}}", version)
    .replaceAll("{{appGroup}}", group)
  writeFileSync(join(contents, "Info.plist"), info)
  const groups = group
    ? `<key>com.apple.security.application-groups</key><array><string>${group}</string></array>`
    : ""
  const exception = group
    ? ""
    : "<key>com.apple.security.temporary-exception.files.home-relative-path.read-write</key><array><string>/Library/Application Support/app.iywclaw/share-inbox/</string></array>"
  const extensionEntitlements = join(root, "build/share.entitlements")
  writeFileSync(
    extensionEntitlements,
    entitlements(
      `<key>com.apple.security.app-sandbox</key><true/>${groups}${exception}`
    )
  )
  writeFileSync(join(root, "build/main.entitlements"), entitlements(groups))
}

function signExtension({ root, bundle, identity }) {
  run("codesign", [
    "--force",
    "--options",
    "runtime",
    ...(identity === "-" ? [] : ["--timestamp"]),
    "--sign",
    identity,
    "--entitlements",
    join(root, "build/share.entitlements"),
    bundle,
  ])
  run("codesign", ["--verify", "--strict", bundle])
}

export function prepareMacShare(target, root) {
  if (process.platform !== "darwin")
    throw new Error("Build the share extension on macOS")
  const identity =
    process.env.IYW_SHARE_UNSIGNED === "1"
      ? "-"
      : process.env.APPLE_SIGNING_IDENTITY || "-"
  const team = process.env.APPLE_TEAM_ID || ""
  if (identity !== "-" && !/^[A-Z0-9]{10}$/.test(team))
    throw new Error(
      "Signed share extensions require the existing APPLE_TEAM_ID"
    )
  const group = identity !== "-" && team ? `${team}.app.iywclaw.share` : ""
  const version = JSON.parse(
    readFileSync(join(root, "../../package.json"), "utf8")
  ).version
  const bundle = join(root, "build/IywAssistantShare.appex")
  const context = { target, root, bundle, version, group, identity }
  compileExtension(context)
  writeMetadata(context)
  signExtension(context)
  writeFileSync(
    join(root, "resources/config.json"),
    JSON.stringify({ schemaVersion: 1, appGroup: group || null })
  )
  console.log(`[native-share] macOS extension prepared for ${target}`)
}
