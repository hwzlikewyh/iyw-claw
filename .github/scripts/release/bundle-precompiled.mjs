import { execFileSync } from "node:child_process"
import { existsSync } from "node:fs"
import { join } from "node:path"
import { runObserved } from "./observe-command.mjs"
import { fixDebControl } from "./linux-deb-control.mjs"

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
// 使用已安装的正式 CLI；各包复用同一个版本，避免每个格式重新 dlx。
const cli = join("node_modules", "@tauri-apps", "cli", "tauri.js")
if (!existsSync(cli)) throw new Error("installed Tauri CLI is missing")
// 修复旧版 rpm-rs 的 gzip 压缩瓶颈；也可用于固定在旧 tag 的修复打包。
if (target.includes("linux") && args[args.indexOf("--bundles") + 1] === "rpm")
  args.push("--config", JSON.stringify({bundle:{linux:{rpm:{compression:{type:"zstd",level:1}}}}}))
await runObserved(process.execPath, [cli, "bundle", ...args], {
  label: `bundle ${target} ${args[args.indexOf("--bundles") + 1] || "default"}`,
})
if (target.includes("linux") && args[args.indexOf("--bundles") + 1] === "deb")
  fixDebControl(target)
