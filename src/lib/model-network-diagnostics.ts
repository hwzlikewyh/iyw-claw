import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import {
  getShellTransport,
  getTransport,
  isDesktop,
  isRemoteDesktopMode,
} from "@/lib/transport"
import type { Transport } from "@/lib/transport"

const DIAGNOSTIC_CACHE_MS = 60_000
const PROBE_TIMEOUT_MS = 10_000
const SLOW_LATENCY_MS = 500

export type NetworkFailureKind =
  | "dns"
  | "timeout"
  | "tls"
  | "refused"
  | "reset"
  | "unavailable"

export interface NetworkProbeStatus {
  target: "internet" | "app_server" | "server_outbound"
  status: "healthy" | "slow" | "failed" | "unknown"
  elapsedMs?: number
  failure?: NetworkFailureKind
}

export interface ModelNetworkDiagnosis {
  client: NetworkProbeStatus
  appServer: NetworkProbeStatus | null
  server: NetworkProbeStatus | null
  applicationProxy: "enabled" | "disabled" | "unknown"
}

interface CachedDiagnosis {
  startedAt: number
  promise: Promise<ModelNetworkDiagnosis>
  evictionTimer: ReturnType<typeof setTimeout>
}

const diagnosisCache = new Map<string, CachedDiagnosis>()

export function getModelNetworkDiagnosis(
  contextKey: string
): Promise<ModelNetworkDiagnosis> {
  const cached = diagnosisCache.get(contextKey)
  if (cached && Date.now() - cached.startedAt < DIAGNOSTIC_CACHE_MS) {
    return cached.promise
  }

  const startedAt = Date.now()
  const promise = collectDiagnosis()
  const evictionTimer = setTimeout(() => {
    if (diagnosisCache.get(contextKey)?.startedAt === startedAt) {
      diagnosisCache.delete(contextKey)
    }
  }, DIAGNOSTIC_CACHE_MS)
  const cachedDiagnosis = { startedAt, promise, evictionTimer }
  diagnosisCache.set(contextKey, cachedDiagnosis)
  return promise
}

export function clearModelNetworkDiagnosis(contextKey: string): void {
  const cached = diagnosisCache.get(contextKey)
  if (!cached) return
  clearTimeout(cached.evictionTimer)
  diagnosisCache.delete(contextKey)
}

async function collectDiagnosis(): Promise<ModelNetworkDiagnosis> {
  const transport = getTransport()
  const desktop = isDesktop()
  const remoteDesktop = isRemoteDesktopMode()
  const localTransport = desktop ? getShellTransport() : transport
  const clientProbe = desktop
    ? measureInternet(localTransport)
    : measureAppServer(transport)
  const appServerProbe = remoteDesktop
    ? measureAppServer(transport)
    : Promise.resolve(null)
  const serverProbe =
    desktop && !remoteDesktop ? Promise.resolve(null) : measureServer(transport)
  const proxyProbe = desktop
    ? readApplicationProxy(localTransport)
    : Promise.resolve("unknown" as const)

  const [client, appServer, server, applicationProxy] = await Promise.all([
    clientProbe,
    appServerProbe,
    serverProbe,
    proxyProbe,
  ])
  return { client, appServer, server, applicationProxy }
}

async function measureInternet(
  transport: Transport
): Promise<NetworkProbeStatus> {
  try {
    const result = await transport.call<{ elapsed_ms: number }>(
      "measure_network_speed",
      { kind: "latency" },
      { timeoutMs: PROBE_TIMEOUT_MS }
    )
    return statusFromLatency("internet", result.elapsed_ms)
  } catch (error) {
    return failedProbe("internet", error)
  }
}

async function measureAppServer(
  transport: Transport
): Promise<NetworkProbeStatus> {
  const startedAt = performance.now()
  try {
    await transport.call("health", {}, { timeoutMs: PROBE_TIMEOUT_MS })
    return statusFromLatency("app_server", performance.now() - startedAt)
  } catch (error) {
    return failedProbe("app_server", error)
  }
}

async function measureServer(
  transport: Transport
): Promise<NetworkProbeStatus> {
  try {
    const result = await transport.call<{ elapsed_ms: number }>(
      "measure_network_speed",
      { kind: "latency" },
      { timeoutMs: PROBE_TIMEOUT_MS }
    )
    return statusFromLatency("server_outbound", result.elapsed_ms)
  } catch (error) {
    if (!extractAppCommandError(error)) {
      return { target: "server_outbound", status: "unknown" }
    }
    return failedProbe("server_outbound", error)
  }
}

function statusFromLatency(
  target: NetworkProbeStatus["target"],
  elapsedMs: number
): NetworkProbeStatus {
  return {
    target,
    status: elapsedMs > SLOW_LATENCY_MS ? "slow" : "healthy",
    elapsedMs: Math.round(elapsedMs),
  }
}

function failedProbe(
  target: NetworkProbeStatus["target"],
  error: unknown
): NetworkProbeStatus {
  return { target, status: "failed", failure: classifyNetworkFailure(error) }
}

function classifyNetworkFailure(error: unknown): NetworkFailureKind {
  const detail = extractAppCommandError(error)?.detail ?? toErrorMessage(error)
  if (/dns|name resolution|no such host|could not resolve/i.test(detail)) {
    return "dns"
  }
  if (/certificate|\btls\b|\bssl\b|handshake/i.test(detail)) return "tls"
  if (/timed? out|\btimeout\b|deadline exceeded/i.test(detail)) {
    return "timeout"
  }
  if (/connection refused|actively refused/i.test(detail)) return "refused"
  if (/connection reset|connection closed|broken pipe/i.test(detail)) {
    return "reset"
  }
  return "unavailable"
}

async function readApplicationProxy(
  transport: Transport
): Promise<ModelNetworkDiagnosis["applicationProxy"]> {
  try {
    const settings = await transport.call<{ enabled?: unknown }>(
      "get_system_proxy_settings",
      {},
      { timeoutMs: PROBE_TIMEOUT_MS }
    )
    if (settings.enabled === true) return "enabled"
    if (settings.enabled === false) return "disabled"
  } catch {
    return "unknown"
  }
  return "unknown"
}
