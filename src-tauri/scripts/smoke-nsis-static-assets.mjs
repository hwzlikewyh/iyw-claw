#!/usr/bin/env node

import { createHash, randomUUID } from "node:crypto"
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import {
  collectRuntimeLogs,
  findOpenPort,
  installPackage,
  launchDesktop,
  resolveInstalledApp,
  stopDesktop,
  waitForDatabase,
  writeWebConfig,
} from "./nsis-smoke-support.mjs"
import { verifyInstalledWeb } from "./nsis-smoke-verification.mjs"
import {
  assertCleanInstallState,
  cleanupInstall,
} from "./nsis-smoke-windows.mjs"

const TEMP_PREFIX = "iyw-claw-nsis-smoke-"
const FAILURE_EXIT_CODE = 1

function fail(message) {
  throw new Error(message)
}

function parseArgs(argv) {
  const args = {
    installer: null,
    output: null,
    sourceSha: "",
    staticRoot: null,
  }
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index]
    const value = argv[index + 1]
    if (!value || value.startsWith("--")) fail(`${flag} needs a value`)
    if (flag === "--installer") args.installer = resolve(value)
    else if (flag === "--output") args.output = resolve(value)
    else if (flag === "--source-sha") args.sourceSha = value
    else if (flag === "--static-root") args.staticRoot = resolve(value)
    else fail(`unknown argument: ${flag}`)
  }
  validateArgs(args)
  return args
}

function validateArgs(args) {
  for (const key of ["installer", "output", "staticRoot"]) {
    if (!args[key])
      fail(
        `--${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)} is required`
      )
  }
  if (!existsSync(args.installer) || !lstatSync(args.installer).isFile()) {
    fail(`installer not found: ${args.installer}`)
  }
  if (
    !existsSync(args.staticRoot) ||
    !lstatSync(args.staticRoot).isDirectory()
  ) {
    fail(`static root not found: ${args.staticRoot}`)
  }
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex")
}

function createContext(args) {
  const testId = randomUUID().replaceAll("-", "")
  const smokeRoot = join(tmpdir(), `${TEMP_PREFIX}${testId}`)
  const installRoot = smokeRoot
  const stateRoot = join(smokeRoot, "state")
  const dataDir = join(stateRoot, "data")
  const logDir = join(stateRoot, "logs")
  mkdirSync(smokeRoot)
  mkdirSync(args.output, { recursive: true })
  return {
    args,
    dataDir,
    installRoot,
    logDir,
    processes: [],
    result: {
      installerBytes: lstatSync(args.installer).size,
      installerSha256: sha256(readFileSync(args.installer)),
      sourceSha: args.sourceSha,
      startedAt: new Date().toISOString(),
      success: false,
    },
    smokeRoot,
    stateRoot,
    testId,
  }
}

function desktopEnvironment(context) {
  return {
    ...process.env,
    IYW_CLAW_DATA_DIR: context.dataDir,
    IYW_CLAW_HOME: context.dataDir,
    IYW_CLAW_LOG_DIR: context.logDir,
    IYW_CLAW_USER_MEMORY_DIR: join(context.stateRoot, "user-memory"),
  }
}

async function executeSmoke(context) {
  const { args } = context
  installPackage({
    installer: args.installer,
    installRoot: context.installRoot,
    logPath: join(args.output, "installer.log"),
    testId: context.testId,
  })
  const appDir = resolveInstalledApp(context.installRoot)
  const executable = join(appDir, "iyw-claw.exe")
  const environment = desktopEnvironment(context)
  const first = launchDesktop({
    environment,
    executable,
    logPath: join(args.output, "first-launch.log"),
    workingDirectory: appDir,
  })
  context.processes.push(first)
  const databasePath = join(context.dataDir, "iyw-claw.db")
  await waitForDatabase({ child: first, databasePath })
  await stopDesktop(first)
  const port = await findOpenPort()
  const token = `iyw-claw-smoke-${randomUUID()}`
  writeWebConfig({ databasePath, port, token })
  const second = launchDesktop({
    environment,
    executable,
    logPath: join(args.output, "second-launch.log"),
    workingDirectory: appDir,
  })
  context.processes.push(second)
  const baseUrl = `http://127.0.0.1:${port}`
  Object.assign(
    context.result,
    await verifyInstalledWeb({
      baseUrl,
      child: second,
      staticRoot: args.staticRoot,
      token,
    })
  )
  context.result.port = port
}

function writeResult(context) {
  context.result.finishedAt = new Date().toISOString()
  writeFileSync(
    join(context.args.output, "smoke-results.json"),
    `${JSON.stringify(context.result, null, 2)}\n`
  )
  const summary = process.env.GITHUB_STEP_SUMMARY
  if (!summary) return
  const status = context.result.success ? "passed" : "failed"
  const lines = [
    "## Installed zlib static asset smoke",
    `- Source: \`${context.result.sourceSha || "unknown"}\``,
    `- Status: **${status}**`,
    `- Static assets checked: ${context.result.staticAssets?.length ?? 0}`,
    "",
  ]
  writeFileSync(summary, `${lines.join("\n")}\n`, { flag: "a" })
}

async function finalize(context) {
  const cleanupErrors = []
  for (const child of [...context.processes].reverse()) {
    try {
      await stopDesktop(child)
    } catch (error) {
      cleanupErrors.push(error.message)
    }
  }
  try {
    collectRuntimeLogs(
      context.logDir,
      join(context.args.output, "runtime-logs")
    )
  } catch (error) {
    cleanupErrors.push(error.message)
  }
  try {
    cleanupErrors.push(...cleanupInstall(context))
  } catch (error) {
    cleanupErrors.push(error.message)
  }
  return cleanupErrors
}

async function main() {
  if (process.platform !== "win32")
    fail("installed NSIS smoke requires Windows")
  const args = parseArgs(process.argv.slice(2))
  assertCleanInstallState()
  const context = createContext(args)
  let failure
  try {
    await executeSmoke(context)
  } catch (error) {
    failure = error
    context.result.error = error.stack ?? error.message
  } finally {
    const cleanupErrors = await finalize(context)
    if (cleanupErrors.length > 0) {
      context.result.cleanupErrors = cleanupErrors
      failure ??= new Error(`smoke cleanup failed: ${cleanupErrors.join("; ")}`)
      context.result.error ??= failure.stack ?? failure.message
    }
    context.result.success = !failure
    writeResult(context)
  }
  if (failure) throw failure
}

main().catch((error) => {
  console.error(`[smoke-nsis-static-assets][ERROR] ${error.stack ?? error}`)
  process.exitCode = FAILURE_EXIT_CODE
})
