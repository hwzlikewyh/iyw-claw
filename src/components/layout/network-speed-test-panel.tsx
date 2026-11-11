"use client"

import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  formatNetworkRate,
  NETWORK_PROBE_KINDS,
} from "@/lib/network-diagnostics"
import type {
  NetworkProbeKind,
  NetworkProbeOutcome,
  NetworkRates,
  NetworkSpeedTest,
} from "@/lib/network-diagnostics"

export interface NetworkSpeedTestPanelProps {
  rates: NetworkRates | null
  error: string | null
  empty: boolean
  test: NetworkSpeedTest
  onStart: () => void
}

export function NetworkSpeedTestPanel(props: NetworkSpeedTestPanelProps) {
  const t = useTranslations("NetworkDiagnostics")
  return (
    <>
      <div className="space-y-1">
        <h2 className="font-medium">{t("title")}</h2>
        <p className="text-xs text-muted-foreground">{t("scope")}</p>
      </div>
      <NetworkThroughput {...props} />
      <NetworkSpeedTestResults test={props.test} />
      <p className="text-xs leading-relaxed text-muted-foreground">
        {t("targetHint")}
      </p>
      <Button
        onClick={props.onStart}
        disabled={props.test.phase !== null}
        className="w-full"
      >
        {props.test.phase && (
          <Loader2 aria-hidden className="size-4 animate-spin" />
        )}
        {props.test.phase
          ? t("testing", { phase: t(props.test.phase) })
          : t("start")}
      </Button>
    </>
  )
}

function NetworkThroughput({
  rates,
  error,
  empty,
}: NetworkSpeedTestPanelProps) {
  const t = useTranslations("NetworkDiagnostics")
  return (
    <div className="space-y-2">
      <p className="text-xs text-muted-foreground">{t("throughput")}</p>
      <p className="tabular-nums">
        {rates
          ? `↓ ${formatNetworkRate(rates.download)}  ↑ ${formatNetworkRate(rates.upload)} Mbps`
          : t(error || empty ? "unavailable" : "sampling")}
      </p>
      {rates && (
        <p className="text-xs text-muted-foreground break-all">{rates.name}</p>
      )}
      {(error || empty) && (
        <p className="text-xs text-destructive break-words">
          {error ?? t("noInterfaces")}
        </p>
      )}
    </div>
  )
}

function NetworkSpeedTestResults({ test }: { test: NetworkSpeedTest }) {
  const t = useTranslations("NetworkDiagnostics")
  return (
    <div className="space-y-2" aria-live="polite" aria-atomic="true">
      <p className="text-xs text-muted-foreground">{t("measurement")}</p>
      <dl className="space-y-2">
        {NETWORK_PROBE_KINDS.map((kind) => (
          <NetworkProbeRow
            key={kind}
            kind={kind}
            outcome={test.outcomes[kind]}
            running={test.phase === kind}
          />
        ))}
      </dl>
      {test.completedAt && (
        <p className="text-xs text-muted-foreground">
          {t("completed", {
            time: new Date(test.completedAt).toLocaleTimeString(),
          })}
        </p>
      )}
    </div>
  )
}

function NetworkProbeRow({
  kind,
  outcome,
  running,
}: {
  kind: NetworkProbeKind
  outcome?: NetworkProbeOutcome
  running: boolean
}) {
  const t = useTranslations("NetworkDiagnostics")
  const value = outcome?.result
  const display = value
    ? kind === "latency"
      ? `${value.elapsed_ms.toFixed(0)} ms`
      : `${formatNetworkRate(value.mbps ?? 0)} Mbps`
    : t(running ? "running" : outcome?.error ? "failed" : "notMeasured")
  return (
    <div>
      <div className="flex items-center justify-between gap-4">
        <dt>{t(kind)}</dt>
        <dd className="tabular-nums">{display}</dd>
      </div>
      {outcome?.error && (
        <p className="mt-1 text-xs text-destructive break-words">
          {outcome.error}
        </p>
      )}
    </div>
  )
}
