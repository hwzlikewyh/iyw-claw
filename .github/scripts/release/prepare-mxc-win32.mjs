import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { readFileSync, realpathSync, writeFileSync } from "node:fs"
import { dirname, join, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"

const REVISION = "b21120168d70befe12356411423266bfef168446"
const PATCHES = [
  {
    file: "src/backends/process_container/common/proxy_coordinator.rs",
    variable: "shell_info",
    before: "c9898eb57383faa3b88e3b25416bbdaf6ecd2db49af6f7e7dd2fec2407e012ef",
    after: "5f89231f2f5505ac0f1db079545cd63a70b2c2c8931eae6e6dbe5970b66ec9d1",
    result: "OwnedHandle::new(shell_info.hProcess)",
    replacement: "OwnedHandle::new(process_handle)",
  },
  {
    file: "src/core/plm/elevated.rs",
    variable: "info",
    before: "676af2e3eeee1e3ca03144424e2b95d96a3ef098fc6d425f964a0305027777b2",
    after: "1dc5eb8080ecca1a795d6fe082a82d6d2eebc33a5a2c2ddb0899e100f8c62fd9",
    result: "info.hProcess",
    replacement: "process_handle",
  },
]
const METADATA_MAX_BYTES = 64 * 1024 * 1024

function digest(source) {
  return createHash("sha256").update(source).digest("hex")
}

function preparePatch(root, patch) {
  const path = join(root, patch.file)
  const source = readFileSync(path, "utf8").replace(/\r\n/g, "\n")
  const actual = digest(source)
  if (actual === patch.after) return { path, changed: false }
  if (actual !== patch.before)
    throw new Error(`MXC source does not match pinned input: ${patch.file}`)
  // 先复制 packed 字段，保留原有校验与句柄所有权，避免创建未对齐引用。
  const patched = source
    .replace(
      `    if ${patch.variable}.hProcess.is_invalid() {`,
      `    let process_handle = ${patch.variable}.hProcess;\n    if process_handle.is_invalid() {`
    )
    .replace(`    Ok(${patch.result})`, `    Ok(${patch.replacement})`)
  if (digest(patched) !== patch.after)
    throw new Error(`MXC patch output mismatch: ${patch.file}`)
  return { path, source: patched, changed: true }
}

export function applyMxcWin32Patch(root) {
  // 两份输入都通过校验后再写入；已修复的源文件可以直接复用。
  const prepared = PATCHES.map((patch) => preparePatch(root, patch))
  for (const patch of prepared) {
    if (patch.changed) writeFileSync(patch.path, patch.source, "utf8")
  }
  return prepared.filter((patch) => patch.changed).length
}

function main() {
  if (
    process.env.GITHUB_ACTIONS !== "true" ||
    process.env.RUNNER_ENVIRONMENT !== "github-hosted" ||
    process.env.TAURI_TARGET_TRIPLE !== "i686-win7-windows-msvc"
  )
    throw new Error("MXC compatibility preparation requires an isolated Win7 x86 CI job")
  const metadata = JSON.parse(
    execFileSync(
      "cargo",
      [
        "metadata",
        "--manifest-path", "src-tauri/Cargo.toml",
        "--format-version", "1",
        "--filter-platform", process.env.TAURI_TARGET_TRIPLE,
      ],
      {
        encoding: "utf8",
        maxBuffer: METADATA_MAX_BYTES,
        stdio: ["ignore", "pipe", "inherit"],
      }
    )
  )
  const sdk = metadata.packages.find((pkg) => pkg.name === "mxc-sdk")
  if (
    sdk?.version !== "1.0.0" ||
    sdk.source !== `git+https://github.com/microsoft/mxc?rev=${REVISION}#${REVISION}`
  )
    throw new Error("MXC dependency does not match the pinned SDK revision")
  const cache = realpathSync(join(process.env.CARGO_HOME, "git", "checkouts"))
  const root = realpathSync(dirname(sdk.manifest_path))
  if (!root.toLowerCase().startsWith(`${cache}${sep}`.toLowerCase()))
    throw new Error("MXC source is outside the isolated Cargo checkout cache")
  const changed = applyMxcWin32Patch(root)
  console.log(`[mxc-win32] verified ${REVISION}; repaired ${changed} packed handle accesses`)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main()
  } catch (error) {
    console.error(`[mxc-win32] ${error.message}`)
    process.exitCode = 1
  }
}
