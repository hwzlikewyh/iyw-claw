import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { lstatSync, readFileSync } from "node:fs"
import { windowsRuntimeImports } from "./xinghe-worker-binary.mjs"

const STATIC_CRT_FLAGS = ["-C", "target-feature=+crt-static"]

export function environmentHelperHostTarget() {
  const output = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  const line = output.split(/\r?\n/).find((value) => value.startsWith("host:"))
  if (!line) throw new Error("rustc did not report a host target")
  return line.slice("host:".length).trim()
}

export function environmentHelperBuildOptions(
  target,
  environment = process.env
) {
  const env = { ...environment }
  const args = []
  if (!target.endsWith("-windows-msvc")) return { args, env }
  // Cargo 优先采用环境变量中的 flags；仅对本次 helper 构建追加静态运行库。
  if (env.CARGO_ENCODED_RUSTFLAGS !== undefined) {
    env.CARGO_ENCODED_RUSTFLAGS = [
      env.CARGO_ENCODED_RUSTFLAGS,
      ...STATIC_CRT_FLAGS,
    ]
      .filter(Boolean)
      .join("\x1f")
  } else if (env.RUSTFLAGS !== undefined) {
    env.RUSTFLAGS = [env.RUSTFLAGS, ...STATIC_CRT_FLAGS]
      .filter(Boolean)
      .join(" ")
  } else {
    args.push(
      "--config",
      `target.${target}.rustflags=${JSON.stringify(STATIC_CRT_FLAGS)}`
    )
  }
  return { args, env }
}

export function verifyEnvironmentRuntime(path, target) {
  if (!target.endsWith("-windows-msvc")) return
  const imports = windowsRuntimeImports(readFileSync(path), target)
  if (imports.length > 0) {
    throw new Error(
      `environment helper must use static MSVC runtime: ${path}; imports=${imports.join(", ")}`
    )
  }
}

export function helperFileName(target) {
  return `iyw-environment-${target}${target.includes("windows") ? ".exe" : ""}`
}

export function helperExecutableName(target) {
  return target.includes("windows") ? "iyw-environment.exe" : "iyw-environment"
}

export function verifyEnvironmentHelper(path, target, version) {
  const stats = lstatSync(path)
  if (!stats.isFile() || stats.size === 0)
    throw new Error(`environment helper must be a non-empty file: ${path}`)
  const digest = createHash("sha256").update(readFileSync(path)).digest("hex")
  console.log(
    `[verify-sidecar-bundle] environment helper: path=${path} version=${version} size=${stats.size} sha256=${digest}`
  )
  verifyEnvironmentRuntime(path, target)
  const output = execFileSync(path, ["--version"], {
    encoding: "utf8",
    windowsHide: true,
  }).trim()
  if (!/^iyw-environment \d+\.\d+\.\d+$/.test(output))
    throw new Error(`environment helper returned an invalid version: ${output}`)
}
