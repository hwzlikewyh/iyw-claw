import type { Transport } from "@/lib/transport"

export const NETWORK_SAMPLE_INTERVAL_MS = 60_000
export const NETWORK_LATENCY_INITIAL_STAGGER_MS = NETWORK_SAMPLE_INTERVAL_MS
const MAX_SAMPLE_GAP_MS = 15_000
const BITS_PER_MEGABIT = 1_000_000

export const NETWORK_PROBE_KINDS = ["latency", "download", "upload"] as const
export type NetworkProbeKind = (typeof NETWORK_PROBE_KINDS)[number]

export interface NetworkInterfaceTraffic {
  name: string
  received_bytes: number
  transmitted_bytes: number
}

export interface NetworkTrafficSnapshot {
  sampled_at_ms: number
  interfaces: NetworkInterfaceTraffic[]
}

export interface NetworkRates {
  name: string
  download: number
  upload: number
}

export interface NetworkProbeResult {
  kind: NetworkProbeKind
  target: string
  elapsed_ms: number
  bytes: number
  mbps: number | null
}

export interface NetworkProbeOutcome {
  result?: NetworkProbeResult
  error?: string
}

export interface NetworkSpeedTest {
  transport: Transport | null
  phase: NetworkProbeKind | null
  outcomes: Partial<Record<NetworkProbeKind, NetworkProbeOutcome>>
  completedAt: number | null
}

export function readNetworkTraffic(transport: Transport) {
  return transport.call<NetworkTrafficSnapshot>("get_network_traffic")
}

export function measureNetworkSpeed(
  transport: Transport,
  kind: NetworkProbeKind
) {
  return transport.call<NetworkProbeResult>("measure_network_speed", { kind })
}

export function calculateNetworkRates(
  previous: NetworkTrafficSnapshot | null,
  current: NetworkTrafficSnapshot,
  preferredName?: string
): NetworkRates | null {
  if (!previous) return null
  const elapsedMs = current.sampled_at_ms - previous.sampled_at_ms
  if (elapsedMs <= 0 || elapsedMs > MAX_SAMPLE_GAP_MS) return null
  const previousByName = new Map(
    previous.interfaces.map((row) => [row.name, row])
  )
  const rates = current.interfaces.flatMap((row) => {
    const before = previousByName.get(row.name)
    if (!before) return []
    const received = row.received_bytes - before.received_bytes
    const sent = row.transmitted_bytes - before.transmitted_bytes
    if (received < 0 || sent < 0) return []
    const factor = (8 * 1_000) / elapsedMs / BITS_PER_MEGABIT
    return [
      { name: row.name, download: received * factor, upload: sent * factor },
    ]
  })
  // 选取当前吞吐最高的单个接口，避免 VPN/网桥与物理网卡重复累加。
  rates.sort((left, right) => {
    const difference =
      right.download + right.upload - left.download - left.upload
    return (
      difference ||
      Number(right.name === preferredName) - Number(left.name === preferredName)
    )
  })
  return rates[0] ?? null
}

export function formatNetworkRate(value: number) {
  return value.toFixed(value >= 10 ? 1 : 2)
}
