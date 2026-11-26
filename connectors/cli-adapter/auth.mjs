import { setTimeout as delay } from "node:timers/promises"
import { execute } from "./process.mjs"
import { prepare, ready, resolveCommand } from "./runtime.mjs"
import { clearAuthDirectories } from "./logout.mjs"

const LOGIN_TIMEOUT_MS = 290_000
const STATUS_TIMEOUT_MS = 10_000
const STATUS_POLL_MS = 2_000

function jsonMatches(value, expected) {
  return Object.entries(expected).every(([key, desired]) => {
    const actual = key.split(".").reduce((item, part) => item?.[part], value)
    return actual !== undefined && String(actual) === String(desired)
  })
}

export async function authenticated(context) {
  if (!(await ready(context))) return false
  const result = await execute(
    context,
    await resolveCommand(context, context.manifest.status),
    { timeoutMs: STATUS_TIMEOUT_MS }
  )
  if (result.code !== 0) return false
  if (context.manifest.statusMatchJson) {
    try {
      const raw = JSON.parse(result.stdout)
      const status = raw?.[context.manifest.statusJsonRoot] ?? raw
      const rejected = context.manifest.statusRejectJson
      return (
        jsonMatches(status, context.manifest.statusMatchJson) &&
        !(rejected && jsonMatches(status, rejected))
      )
    } catch {
      return false
    }
  }
  if (context.manifest.statusMatch)
    return new RegExp(context.manifest.statusMatch).test(result.stdout)
  return true
}

export async function login(context) {
  const deadline = Date.now() + LOGIN_TIMEOUT_MS
  await prepare(context)
  if (await authenticated(context)) return 0
  for (const step of context.manifest.login) {
    if (step.skipIf) {
      const check = await execute(
        context,
        await resolveCommand(context, step.skipIf),
        { timeoutMs: STATUS_TIMEOUT_MS }
      )
      if (check.code === 0) continue
    }
    const remaining = deadline - Date.now()
    if (remaining <= 0)
      throw new Error("安装或登录耗时较长，环境已保留，请再次登录")
    const result = await execute(
      context,
      await resolveCommand(context, step.command),
      { inherit: true, timeoutMs: remaining }
    )
    if (!(step.successCodes ?? [0]).includes(result.code)) {
      throw new Error(`连接器登录失败（退出码 ${result.code}）`)
    }
  }
  console.error("请完成服务方页面授权，正在等待登录结果…")
  while (Date.now() < deadline) {
    if (await authenticated(context)) return 0
    await delay(STATUS_POLL_MS)
  }
  throw new Error("登录等待超时，请完成授权后重试")
}

export async function logout(context) {
  if (!(await ready(context))) return 0
  for (const command of context.manifest.logout ?? []) {
    const result = await execute(
      context,
      await resolveCommand(context, command),
      { timeoutMs: STATUS_TIMEOUT_MS }
    )
    if (result.code !== 0)
      throw new Error(`连接器断开失败（退出码 ${result.code}）`)
  }
  await clearAuthDirectories(context)
  return (await authenticated(context)) ? 1 : 0
}
