import { execFileSync } from "node:child_process"
import { join } from "node:path"

export function buildWorker(root, target) {
  const args = [
    "build",
    "--manifest-path",
    join(root, "harness/xinghe-worker/Cargo.toml"),
    "--target-dir",
    join(root, "harness/xinghe-worker/target"),
    "--release",
    "--locked",
    "--timings",
    "--target",
    target,
  ]
  const options = { cwd: root, stdio: "inherit", windowsHide: true }
  execFileSync(
    "cargo",
    [...args, "--lib", "--bin", "iyw-xinghe-helper"],
    options
  )
  if (target.includes("windows")) {
    execFileSync(
      "cargo",
      [...args, "-p", "codex-windows-sandbox", "--bins"],
      options
    )
  }
}
