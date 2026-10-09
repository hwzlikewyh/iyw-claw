import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { lstatSync, readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import {
  environmentHelperHostTarget,
  verifyEnvironmentRuntime,
} from "./environment-helper-runtime.mjs"

export function computerHelperName(target) {
  return `iyw-computer-helper${target.includes("windows") ? ".exe" : ""}`
}

export function computerSourceFingerprint(root) {
  const files = []
  const walk = (directory) => {
    for (const item of readdirSync(join(root, directory), {
      withFileTypes: true,
    })) {
      const path = `${directory}/${item.name}`
      if (item.isDirectory()) walk(path)
      else if (item.name.endsWith(".rs")) files.push(path)
    }
  }
  walk("src/computer")
  walk("src/commands/parts/computer")
  walk("src/acp/parts/computer_tools")
  files.push(
    "src/commands/computer.rs",
    "src/acp/computer_tools.rs",
    "src/bin_targets/iyw_computer_helper.rs"
  )
  const hash = createHash("sha256")
  for (const file of files.sort())
    hash.update(file).update(readFileSync(join(root, file)))
  return hash.digest("hex")
}

export function verifyComputerHelper(path, target, version, source = null) {
  const stat = lstatSync(path)
  if (!stat.isFile() || !stat.size)
    throw new Error(`Computer helper is missing or empty: ${path}`)
  verifyEnvironmentRuntime(path, target)
  if (target !== environmentHelperHostTarget()) return
  const identity = JSON.parse(
    execFileSync(path, ["--identity"], { encoding: "utf8", timeout: 10000 })
  )
  if (
    identity.version !== version ||
    identity.target !== target ||
    (source && identity.source !== source)
  ) {
    throw new Error(
      `Computer helper identity does not match this application: ${path}`
    )
  }
}
