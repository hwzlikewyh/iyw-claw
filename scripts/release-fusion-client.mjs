import { existsSync, readFileSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"

const BASE = "https://gateway.iyw.cn/iyw-fusion-api"
const REQUEST_TIMEOUT = 180_000

async function fetchJson(url, options = {}) {
  let response
  try {
    response = await fetch(url, {
      ...options,
      signal: AbortSignal.timeout(REQUEST_TIMEOUT),
    })
  } catch {
    throw new Error(
      "Fusion request failed or timed out; retry the same version to resume"
    )
  }
  if (!response.ok) throw new Error(`Fusion HTTP ${response.status}`)
  const result = await response.json()
  if (result.code !== 1) {
    const error = new Error(
      `Fusion ${result.code}: ${result.message || "request rejected"}`
    )
    error.code = result.data?.error?.code
    throw error
  }
  return result.data
}

export async function connectFusion(environment = process.env) {
  const admin =
    environment.IYW_FUSION_ADMIN_TOKEN || environment.FUSION_ADMIN_TOKEN
  if (!admin) throw new Error("set IYW_FUSION_ADMIN_TOKEN before releasing")
  const tokenFile = join(homedir(), ".iyw-claw/iyw-account-token.json")
  const local = existsSync(tokenFile)
    ? JSON.parse(readFileSync(tokenFile, "utf8")).access_token
    : null
  const tokens = new Set(
    [
      local,
      environment.IYW_FUSION_GATEWAY_TOKEN,
      environment.FUSION_ACCESS_TOKEN,
    ].filter(Boolean)
  )
  for (const token of tokens) {
    const headers = {
      token,
      "admin-token": admin,
      "X-IYW-Admin-Actor": "desktop-release-script",
    }
    try {
      await fetchJson(`${BASE}/admin/api/app-releases?pageSize=1`, { headers })
      return (path, body) =>
        fetchJson(`${BASE}${path}`, {
          headers: { ...headers, "Content-Type": "application/json" },
          method: body === undefined ? "GET" : "POST",
          ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        })
    } catch (error) {
      if (!/403|401/.test(error.message)) throw error
    }
  }
  throw new Error(
    "Fusion login expired; sign in to 原助理 or update IYW_FUSION_GATEWAY_TOKEN, then retry"
  )
}
