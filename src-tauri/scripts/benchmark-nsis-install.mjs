#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import {
  existsSync,
  lstatSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs"
import { join, relative, resolve } from "node:path"
import { performance } from "node:perf_hooks"
import { tmpdir } from "node:os"

const MAX_RUNS = 6

function fail(message) {
  throw new Error(message)
}

function parseArgs(argv) {
  const args = {
    output: null,
    requireNoDuplicateBundle: false,
    runs: 3,
    sourceSha: "",
    variants: [],
  }
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index]
    const value = argv[index + 1]
    if (flag === "--variant") {
      if (!value || value.startsWith("--")) fail("--variant needs NAME=PATH")
      const separator = value.indexOf("=")
      if (separator < 1 || separator === value.length - 1) {
        fail(`invalid variant: ${value}`)
      }
      const name = value.slice(0, separator)
      const installer = resolve(value.slice(separator + 1))
      if (!/^[a-z0-9-]+$/.test(name)) fail(`invalid variant name: ${name}`)
      args.variants.push({ installer, name })
      index += 1
    } else if (flag === "--runs") {
      const runs = Number.parseInt(value, 10)
      if (!Number.isInteger(runs) || runs < 1 || runs > MAX_RUNS) {
        fail(`--runs must be an integer from 1 to ${MAX_RUNS}`)
      }
      args.runs = runs
      index += 1
    } else if (flag === "--output") {
      if (!value || value.startsWith("--")) fail("--output needs a path")
      args.output = resolve(value)
      index += 1
    } else if (flag === "--source-sha") {
      if (!value || value.startsWith("--")) fail("--source-sha needs a value")
      args.sourceSha = value
      index += 1
    } else if (flag === "--require-no-duplicate-bundle") {
      args.requireNoDuplicateBundle = true
    } else {
      fail(`unknown argument: ${flag}`)
    }
  }
  if (args.variants.length < 2)
    fail("at least two --variant values are required")
  if (!args.output) fail("--output is required")
  const names = new Set(args.variants.map(({ name }) => name))
  if (names.size !== args.variants.length) fail("variant names must be unique")
  return args
}

function sha256(path) {
  if (!existsSync(path) || !lstatSync(path).isFile())
    fail(`missing file: ${path}`)
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

function collectTree(root) {
  const stack = [root]
  const files = []
  let bytes = 0
  while (stack.length > 0) {
    const directory = stack.pop()
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) {
        stack.push(path)
      } else if (entry.isFile()) {
        const size = lstatSync(path).size
        files.push({ path, size })
        bytes += size
      }
    }
  }
  return { bytes, files }
}

function resolveInstalledApp(root) {
  const candidates = [join(root, "app"), root]
  const app = candidates.find((candidate) =>
    existsSync(join(candidate, "iyw-claw.exe"))
  )
  if (!app) fail(`installed iyw-claw.exe not found below ${root}`)
  return app
}

function executableInventory(app, files) {
  return files
    .filter(({ path }) => path.toLowerCase().endsWith(".exe"))
    .map(({ path, size }) => ({
      bytes: size,
      path: relative(app, path).replaceAll("\\", "/"),
      sha256: sha256(path),
    }))
    .sort((left, right) => left.path.localeCompare(right.path))
}

function runInstall(variant, round, output, requireNoDuplicateBundle) {
  const root = mkdtempSync(join(tmpdir(), `iyw-claw-nsis-${variant.name}-`))
  try {
    const started = performance.now()
    execFileSync(variant.installer, ["/S", `/D=${root}`], {
      stdio: "ignore",
    })
    const elapsedMs = Math.round(performance.now() - started)
    const app = resolveInstalledApp(root)
    const tree = collectTree(app)
    const duplicateBundle = existsSync(join(app, "resources", "bundle"))
    if (duplicateBundle && requireNoDuplicateBundle)
      fail(`${variant.name}: duplicate resources/bundle remains`)
    const mainExecutable = join(app, "iyw-claw.exe")
    const result = {
      compression: variant.name,
      elapsedMs,
      executables: executableInventory(app, tree.files),
      files: tree.files.length,
      duplicateBundle,
      round,
      installedBytes: tree.bytes,
      mainExeSha256: sha256(mainExecutable),
    }
    output.runs.push(result)
    return result
  } finally {
    rmSync(root, { force: true, recursive: true })
  }
}

function median(values) {
  const sorted = [...values].sort((left, right) => left - right)
  return sorted[Math.floor(sorted.length / 2)]
}

function writeSummary(result, outputPath) {
  mkdirSync(outputPath, { recursive: true })
  writeFileSync(
    join(outputPath, "install-results.json"),
    `${JSON.stringify(result, null, 2)}\n`
  )
  const lines = [
    "## Windows NSIS install benchmark",
    `- Source: \`${result.sourceSha || "unknown"}\``,
    `- Runs per variant: ${result.runsPerVariant}`,
    "",
    "| Compression | Installer bytes | Median install ms | Files | Installed bytes |",
    "| --- | ---: | ---: | ---: | ---: |",
  ]
  for (const variant of result.variants) {
    lines.push(
      `| ${variant.compression} | ${variant.installerBytes} | ${variant.medianInstallMs} | ${variant.files} | ${variant.installedBytes} |`
    )
  }
  const summaryPath = process.env.GITHUB_STEP_SUMMARY
  if (summaryPath)
    writeFileSync(summaryPath, `${lines.join("\n")}\n`, { flag: "a" })
}

function main() {
  if (process.platform !== "win32")
    fail("NSIS install benchmark requires Windows")
  const args = parseArgs(process.argv.slice(2))
  const output = {
    sourceSha: args.sourceSha,
    runs: [],
    runsPerVariant: args.runs,
  }
  const orders = []
  for (let round = 0; round < args.runs; round += 1) {
    const order = [...args.variants]
    if (round % 2 === 1) order.reverse()
    orders.push(order)
  }
  for (let round = 0; round < orders.length; round += 1) {
    for (const variant of orders[round]) {
      runInstall(
        variant,
        round + 1,
        output,
        args.requireNoDuplicateBundle
      )
    }
  }
  output.variants = args.variants.map((variant) => {
    const runs = output.runs.filter((run) => run.compression === variant.name)
    const installerBytes = lstatSync(variant.installer).size
    return {
      compression: variant.name,
      installerBytes,
      installedBytes: runs[0].installedBytes,
      files: runs[0].files,
      medianInstallMs: median(runs.map((run) => run.elapsedMs)),
      installMs: runs.map((run) => run.elapsedMs),
      installerSha256: sha256(variant.installer),
      mainExeSha256: runs[0].mainExeSha256,
    }
  })
  writeSummary(output, args.output)
}

try {
  main()
} catch (error) {
  console.error(`[benchmark-nsis-install][ERROR] ${error.message}`)
  process.exitCode = 1
}
