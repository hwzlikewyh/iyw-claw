import { execFileSync } from "node:child_process"
import { readFileSync } from "node:fs"
import {
  verifyHelperBinary,
  windowsRuntimeImports,
} from "./xinghe-worker-binary.mjs"

export function verifyEnvironmentHelper(path, options) {
  const bytes = readFileSync(path)
  if (options.target.includes("windows")) {
    const imports = windowsRuntimeImports(bytes, options.target)
    if (imports.length)
      throw new Error(
        `Environment helper requires external CRT: ${imports.join(", ")}`
      )
  } else {
    verifyHelperBinary(bytes, options.target)
  }
  const os = options.target.includes("windows")
    ? "win32"
    : options.target.includes("darwin")
      ? "darwin"
      : "linux"
  const arch = options.target.startsWith("aarch64") ? "arm64" : "x64"
  if (process.platform !== os || process.arch !== arch) {
    console.log(
      `[environment-helper] architecture verified; native execution pending for ${options.target}`
    )
    return
  }
  const run = (arg) =>
    execFileSync(path, [arg], {
      encoding: "utf8",
      timeout: 10_000,
      windowsHide: true,
    }).trim()
  if (!/^iyw-environment \d+\.\d+\.\d+$/.test(run("--version")))
    throw new Error("Invalid environment helper version")
  if (run("--application-version") !== options.version)
    throw new Error("Environment helper application version mismatch")
}
