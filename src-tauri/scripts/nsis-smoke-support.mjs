import { execFileSync, spawn } from "node:child_process"
import { once } from "node:events"
import { closeSync, cpSync, existsSync, lstatSync, openSync } from "node:fs"
import { createServer } from "node:net"
import { join } from "node:path"
import { DatabaseSync } from "node:sqlite"

import {
  commandDiagnostic,
  installerTestArgs,
  terminateProcessTree,
} from "./nsis-smoke-windows.mjs"

const DATABASE_POLL_MS = 500
const DATABASE_TIMEOUT_MS = 180_000
const PROCESS_STOP_TIMEOUT_MS = 15_000

function fail(message) {
  throw new Error(message)
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds))
}

function assertProcessAlive(child) {
  if (child.spawnError)
    fail(`desktop launch failed: ${child.spawnError.message}`)
  if (child.exitCode !== null || child.signalCode !== null) {
    fail(
      `desktop exited early: code=${child.exitCode}, signal=${child.signalCode}`
    )
  }
}

function databaseInitialized(databasePath) {
  if (!existsSync(databasePath) || !lstatSync(databasePath).isFile())
    return false
  let database
  try {
    database = new DatabaseSync(databasePath, { readOnly: true })
    const row = database
      .prepare(
        "SELECT 1 AS present FROM app_metadata " +
          "WHERE key = 'app_version' AND deleted_at IS NULL"
      )
      .get()
    return Boolean(row?.present)
  } catch {
    return false
  } finally {
    database?.close()
  }
}

export async function waitForDatabase(options) {
  const deadline = Date.now() + DATABASE_TIMEOUT_MS
  while (Date.now() < deadline) {
    assertProcessAlive(options.child)
    if (databaseInitialized(options.databasePath)) return
    await delay(DATABASE_POLL_MS)
  }
  fail(`database migration timed out: ${options.databasePath}`)
}

export function writeWebConfig(options) {
  const database = new DatabaseSync(options.databasePath)
  const statement = database.prepare(
    "INSERT INTO app_metadata " +
      "(key, value, created_at, updated_at, deleted_at) " +
      "VALUES (?, ?, ?, ?, NULL) ON CONFLICT(key) DO UPDATE SET " +
      "value = excluded.value, updated_at = excluded.updated_at, " +
      "deleted_at = NULL"
  )
  const now = new Date().toISOString()
  let transactionStarted = false
  try {
    database.exec("BEGIN IMMEDIATE")
    transactionStarted = true
    statement.run("web_service_token", options.token, now, now)
    statement.run("web_service_port", String(options.port), now, now)
    statement.run("web_service_auto_start", "true", now, now)
    database.exec("COMMIT")
  } catch (error) {
    if (transactionStarted) database.exec("ROLLBACK")
    throw error
  } finally {
    database.close()
  }
}

export function launchDesktop(options) {
  const descriptor = openSync(options.logPath, "a")
  try {
    const child = spawn(options.executable, [], {
      cwd: options.workingDirectory,
      env: options.environment,
      stdio: ["ignore", descriptor, descriptor],
      windowsHide: true,
    })
    child.spawnError = null
    child.on("error", (error) => {
      child.spawnError = error
    })
    return child
  } finally {
    closeSync(descriptor)
  }
}

async function waitForExit(child) {
  if (child.exitCode !== null || child.signalCode !== null) return
  let timeout
  try {
    await Promise.race([
      once(child, "exit"),
      new Promise((_, reject) => {
        timeout = setTimeout(
          () => reject(new Error(`process ${child.pid} did not stop`)),
          PROCESS_STOP_TIMEOUT_MS
        )
      }),
    ])
  } finally {
    clearTimeout(timeout)
  }
}

export async function stopDesktop(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return
  const termination = terminateProcessTree(child.pid)
  try {
    await waitForExit(child)
  } catch (error) {
    fail(`${error.message}; ${commandDiagnostic("taskkill", termination)}`)
  }
}

function runWithLog(file, args, logPath) {
  const descriptor = openSync(logPath, "a")
  try {
    execFileSync(file, args, {
      stdio: ["ignore", descriptor, descriptor],
      windowsHide: true,
    })
  } finally {
    closeSync(descriptor)
  }
}

export function installPackage(options) {
  runWithLog(
    options.installer,
    ["/S", ...installerTestArgs(options.testId), `/D=${options.installRoot}`],
    options.logPath
  )
}

export function resolveInstalledApp(root) {
  const candidates = [join(root, "app"), root]
  const app = candidates.find((path) => existsSync(join(path, "iyw-claw.exe")))
  if (!app) fail(`installed iyw-claw.exe not found below ${root}`)
  return app
}

export function collectRuntimeLogs(source, destination) {
  if (existsSync(source)) cpSync(source, destination, { recursive: true })
}

export function findOpenPort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer()
    server.once("error", reject)
    server.listen(0, "127.0.0.1", () => {
      const address = server.address()
      const port = typeof address === "object" && address ? address.port : 0
      server.close((error) => {
        if (error) reject(error)
        else if (!port) reject(new Error("failed to reserve a port"))
        else resolvePort(port)
      })
    })
  })
}
