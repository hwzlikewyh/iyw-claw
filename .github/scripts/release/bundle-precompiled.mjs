import { execFileSync } from "node:child_process"
import { existsSync } from "node:fs"
import { join } from "node:path"

// tauri-action 固定传入 build；通过其 tauriScript 扩展点只执行官方 bundle。
const [command, ...args] = process.argv.slice(2)
const target = args[args.indexOf("--target") + 1]
const supported = new Set([
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
])
if (command !== "build" || !supported.has(target))
  throw new Error("precompiled bundling requires a supported desktop target")
const separator = args.indexOf("--")
if (separator >= 0) {
  if (args.slice(separator + 1).join(" ") !== "--timings")
    throw new Error("unexpected Cargo arguments in bundle-only phase")
  args.splice(separator)
}
if (!existsSync(join("src-tauri/target", target, "release/iyw-claw")))
  throw new Error(`compiled application is missing: ${target}`)
execFileSync(
  process.execPath,
  ["src-tauri/scripts/verify-xinghe-worker-bundle.mjs", "--target", target],
  { stdio: "inherit" }
)
execFileSync("pnpm", ["dlx", "@tauri-apps/cli@2.11.4", "bundle", ...args], {
  stdio: "inherit",
})
// 上传前执行体积门禁，独立打包的 DEB/RPM 也逐个核验。
execFileSync(
  process.execPath,
  ["src-tauri/scripts/verify-desktop-bundle-size.mjs", "--target", target],
  { stdio: "inherit" }
)
