import { execFileSync } from "node:child_process"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { targetInfo } from "./runtime-seed-config.mjs"

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const HOST_PREFIX = "host:"

export function resolveMacTarget() {
  const configured = process.env.TAURI_TARGET_TRIPLE
  const target =
    configured ||
    execFileSync("rustc", ["-vV"], { encoding: "utf8" })
      .split(/\r?\n/)
      .find((line) => line.startsWith(HOST_PREFIX))
      ?.slice(HOST_PREFIX.length)
      .trim()
  if (!target || targetInfo(target).os !== "macos")
    throw new Error(`Unsupported macOS desktop target: ${target ?? "unknown"}`)
  return target
}

function scriptStep(name, target) {
  return {
    label: name,
    args: [
      join(REPO_ROOT, "src-tauri/scripts", `${name}.mjs`),
      "--target",
      target,
    ],
  }
}

export function createMacBuildPlan(tauriCli, options, target) {
  if (options.authenticode)
    throw new Error("--authenticode is only supported for Windows builds")
  if (targetInfo(target).os !== "macos")
    throw new Error(`Unsupported macOS desktop target: ${target}`)
  const env = { ...process.env, TAURI_TARGET_TRIPLE: target }
  if (options.jobs) env.CARGO_BUILD_JOBS = String(options.jobs)
  const args = [
    tauriCli,
    options.bundleOnly ? "bundle" : "build",
    "--target",
    target,
    "--config",
    join(REPO_ROOT, "src-tauri/tauri.runtime-seed.conf.json"),
  ]
  if (!options.bundleOnly) {
    // sidecar 与 seed 已单独准备；仅控制前端是否复用，避免重复运行构建前钩子。
    args.push(
      "--config",
      JSON.stringify({
        build: {
          beforeBuildCommand: options.reuseAssets ? null : "pnpm build",
        },
      })
    )
  }
  if (options.verbose) args.push("-vv")
  if (options.noSign) args.push("--no-sign")
  if (!options.bundleOnly) args.push("--", "--timings")
  return {
    env,
    steps: [
      scriptStep("prepare-sidecars", target),
      scriptStep("prepare-runtime-seed", target),
      scriptStep("verify-runtime-seed", target),
      {
        label: options.bundleOnly ? "macOS bundle" : "release build and bundle",
        args,
      },
      scriptStep("verify-runtime-seed-bundle", target),
      scriptStep("verify-desktop-bundle-size", target),
    ],
  }
}
