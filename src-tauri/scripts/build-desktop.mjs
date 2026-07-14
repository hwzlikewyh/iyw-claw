#!/usr/bin/env node

import { spawnSync } from "node:child_process"
import { copyFileSync, existsSync, readFileSync, readdirSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

const SCRIPT_PATH = fileURLToPath(import.meta.url)
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..", "..")

export function brandedInstallerName(fileName) {
  const match = /^iyw-claw_([^_]+)_([^-]+)-setup\.exe$/i.exec(fileName)
  if (!match) throw new Error(`unrecognized NSIS installer name: ${fileName}`)
  return `原助手-v${match[1]}-${match[2]}-setup.exe`
}

export function stageBrandedInstallerArtifacts(repoRoot = REPO_ROOT) {
  const packageJson = JSON.parse(
    readFileSync(join(repoRoot, "package.json"), "utf8")
  )
  const outputDir = join(
    repoRoot,
    "src-tauri",
    "target",
    "release",
    "bundle",
    "nsis"
  )
  if (!existsSync(outputDir)) {
    throw new Error(`NSIS output directory is missing: ${outputDir}`)
  }
  const installers = readdirSync(outputDir).filter(
    (fileName) =>
      fileName.startsWith(`iyw-claw_${packageJson.version}_`) &&
      fileName.endsWith("-setup.exe")
  )
  if (installers.length === 0) {
    throw new Error(
      `NSIS installer for v${packageJson.version} was not produced`
    )
  }
  for (const installer of installers) {
    const source = join(outputDir, installer)
    const branded = join(outputDir, brandedInstallerName(installer))
    copyFileSync(source, branded)
    if (existsSync(`${source}.sig`)) {
      copyFileSync(`${source}.sig`, `${branded}.sig`)
    }
    console.log(`[desktop-build] branded installer staged at ${branded}`)
  }
}

export function parseBuildOptions(argv) {
  const options = {
    bundleOnly: false,
    jobs: null,
    noSign: false,
    reuseAssets: false,
    verbose: false,
  }
  for (const arg of argv) {
    if (arg === "--bundle-only") {
      options.bundleOnly = true
    } else if (arg === "--no-sign") {
      options.noSign = true
    } else if (arg === "--reuse-assets") {
      options.reuseAssets = true
    } else if (arg === "--verbose") {
      options.verbose = true
    } else if (arg.startsWith("--jobs=")) {
      const jobs = Number.parseInt(arg.slice("--jobs=".length), 10)
      if (!Number.isInteger(jobs) || jobs < 1) {
        throw new Error(`invalid Cargo job count: ${arg}`)
      }
      options.jobs = jobs
    } else {
      throw new Error(`unknown option: ${arg}`)
    }
  }
  return options
}

export function createBuildPlan(tauriCli, options) {
  const env = { ...process.env }
  if (options.jobs) {
    env.CARGO_BUILD_JOBS = String(options.jobs)
  }

  const bundle = {
    label: "NSIS bundle",
    args: [tauriCli, "bundle", "--bundles", "nsis"],
  }
  if (options.noSign) {
    bundle.args.push("--no-sign")
  }
  if (options.bundleOnly) {
    return { env, steps: [bundle] }
  }

  const buildArgs = [tauriCli, "build"]
  if (options.reuseAssets) {
    buildArgs.push("--config", '{"build":{"beforeBuildCommand":null}}')
  }
  if (options.verbose) {
    buildArgs.push("-vv")
  }
  if (options.noSign) {
    buildArgs.push("--no-sign")
  }
  buildArgs.push("--", "--timings")
  return {
    env,
    steps: [{ label: "release build and bundle", args: buildArgs }],
  }
}

function runStep(step, env) {
  console.log(`[desktop-build] starting ${step.label}`)
  if (step.label === "release build and bundle") {
    console.log(
      "[desktop-build] the final iyw_claw codegen/link unit may take several minutes"
    )
  }
  const result = spawnSync(process.execPath, step.args, {
    cwd: REPO_ROOT,
    env,
    stdio: "inherit",
  })
  if (result.error) {
    throw result.error
  }
  if (result.status !== 0) {
    throw new Error(`${step.label} exited with code ${result.status ?? 1}`)
  }
}

function main() {
  const options = parseBuildOptions(process.argv.slice(2))
  if (options.reuseAssets) {
    console.log(
      "[desktop-build] reusing existing out/ and sidecar assets; run the production build after either changes"
    )
  }
  const tauriCli = join(
    REPO_ROOT,
    "node_modules",
    "@tauri-apps",
    "cli",
    "tauri.js"
  )
  const plan = createBuildPlan(tauriCli, options)
  for (const step of plan.steps) {
    runStep(step, plan.env)
  }
  stageBrandedInstallerArtifacts()
  if (!options.bundleOnly) {
    console.log(
      "[desktop-build] Cargo timing report: src-tauri/target/cargo-timings/cargo-timing.html"
    )
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(SCRIPT_PATH)) {
  try {
    main()
  } catch (error) {
    console.error(`[desktop-build][ERROR] ${error.message}`)
    process.exit(1)
  }
}
