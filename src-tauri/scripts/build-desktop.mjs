#!/usr/bin/env node

import { spawnSync } from "node:child_process"
import { copyFileSync, existsSync, readFileSync, readdirSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

const SCRIPT_PATH = fileURLToPath(import.meta.url)
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..", "..")

export function brandedInstallerName(fileName) {
  const match = /^(?:iyw-claw|原助理)_([^_]+)_([^-]+)-setup\.exe$/i.exec(
    fileName
  )
  if (!match) throw new Error(`unrecognized NSIS installer name: ${fileName}`)
  return `原助理-v${match[1]}-${match[2]}-setup.exe`
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
      [
        `iyw-claw_${packageJson.version}_`,
        `原助理_${packageJson.version}_`,
      ].some((prefix) => fileName.startsWith(prefix)) &&
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
    authenticode: false,
    bundleOnly: false,
    jobs: null,
    noSign: false,
    reuseAssets: false,
    verbose: false,
  }
  for (const arg of argv) {
    if (arg === "--bundle-only") {
      options.bundleOnly = true
    } else if (arg === "--authenticode") {
      // Authenticode (the Windows trust chain) is unrelated to --no-sign,
      // which only controls the updater's minisign signature. The two are
      // independent and may be combined.
      options.authenticode = true
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

/**
 * @param signingConfigPath absolute path to the generated Authenticode overlay
 *        (see prepare-signing-config.mjs), or null for an unsigned build.
 */
export function createBuildPlan(tauriCli, options, signingConfigPath = null) {
  const env = { ...process.env }
  if (options.jobs) {
    env.CARGO_BUILD_JOBS = String(options.jobs)
  }

  const bundle = {
    label: "NSIS bundle",
    args: [tauriCli, "bundle", "--bundles", "nsis"],
  }
  if (signingConfigPath) {
    bundle.args.push("--config", signingConfigPath)
  }
  if (options.noSign) {
    bundle.args.push("--no-sign")
  }
  const prepareSidecars = {
    label: "sidecar preparation",
    args: [join(REPO_ROOT, "src-tauri", "scripts", "prepare-sidecars.mjs")],
  }
  if (options.bundleOnly) {
    return { env, steps: [prepareSidecars, bundle] }
  }

  const buildArgs = [tauriCli, "build"]
  if (options.reuseAssets) {
    buildArgs.push("--config", '{"build":{"beforeBuildCommand":null}}')
  }
  // Later --config wins on conflict, and this one only sets bundle.windows
  // keys, so it composes with the --reuse-assets overlay above.
  if (signingConfigPath) {
    buildArgs.push("--config", signingConfigPath)
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
    steps: [
      ...(options.reuseAssets ? [prepareSidecars] : []),
      { label: "release build and bundle", args: buildArgs },
    ],
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

/**
 * Write the Authenticode overlay and return its path.
 *
 * Signing is driven entirely by the bundler through `signCommand`; this script
 * deliberately does NOT sign the installer afterwards. Tauri computes the
 * updater's minisign `.sig` over the finished installer bytes, so embedding an
 * Authenticode signature after that point would invalidate it and every client
 * would reject the update.
 */
function prepareSigningOverlay() {
  const result = spawnSync(
    process.execPath,
    [join(REPO_ROOT, "src-tauri", "scripts", "prepare-signing-config.mjs")],
    { cwd: REPO_ROOT, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }
  )
  if (result.error) {
    throw result.error
  }
  if (result.status !== 0) {
    throw new Error("Authenticode signing configuration is incomplete")
  }
  const overlay = result.stdout.trim()
  if (!overlay) {
    throw new Error("prepare-signing-config.mjs printed no overlay path")
  }
  return overlay
}

function main() {
  const options = parseBuildOptions(process.argv.slice(2))
  if (options.reuseAssets) {
    console.log(
      "[desktop-build] reusing existing out/ assets; sidecars will be rebuilt"
    )
  }
  const tauriCli = join(
    REPO_ROOT,
    "node_modules",
    "@tauri-apps",
    "cli",
    "tauri.js"
  )
  const signingConfigPath = options.authenticode
    ? prepareSigningOverlay()
    : null
  if (signingConfigPath) {
    console.log(`[desktop-build] Authenticode overlay: ${signingConfigPath}`)
  }
  const plan = createBuildPlan(tauriCli, options, signingConfigPath)
  for (const step of plan.steps) {
    runStep(step, plan.env)
  }
  runStep(
    {
      label: "staged sidecar verification",
      args: [
        join(REPO_ROOT, "src-tauri", "scripts", "verify-sidecar-bundle.mjs"),
      ],
    },
    plan.env
  )
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
