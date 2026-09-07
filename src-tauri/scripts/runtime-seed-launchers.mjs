import { chmod, readFile, stat, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { safeRelativePath } from "./runtime-seed-files.mjs"

const EXECUTABLE_MODE = 0o755

export async function writeNodeLaunchers(root) {
  // Unix 的 npm/npx 原本是符号链接，物化后必须从原包目录加载相对依赖。
  for (const command of ["npm", "npx"]) {
    const entry = `../lib/node_modules/npm/bin/${command}-cli.js`
    await stat(join(root, "bin", entry))
    const launcher = join(root, "bin", command)
    await writeFile(
      launcher,
      `#!/usr/bin/env node\nprocess.argv[1] = require.resolve(${JSON.stringify(entry)})\nrequire(process.argv[1])\n`
    )
    await chmod(launcher, EXECUTABLE_MODE)
  }
}

export async function writeCodexLauncher(root) {
  const packageRoot = join(
    root,
    "node_modules",
    "@agentclientprotocol",
    "codex-acp"
  )
  const manifest = JSON.parse(
    await readFile(join(packageRoot, "package.json"), "utf8")
  )
  const entry = manifest.bin?.["codex-acp"]
  if (!safeRelativePath(entry ?? ""))
    throw new Error("Codex runtime seed launcher entrypoint is invalid")
  await stat(join(packageRoot, entry))
  const relative = `../@agentclientprotocol/codex-acp/${entry}`.replaceAll(
    "'",
    "'\\''"
  )
  const launcher = join(root, "node_modules", ".bin", "codex-acp")
  // exec 保留信号与退出码，并让 Node 从原始入口加载 ESM/CJS 模块。
  await writeFile(
    launcher,
    `#!/bin/sh\nbasedir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1\nexec node "$basedir"/'${relative}' "$@"\n`
  )
  await chmod(launcher, EXECUTABLE_MODE)
}
