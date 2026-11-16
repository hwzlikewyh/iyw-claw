"use client"

import { useState, useSyncExternalStore } from "react"
import { useTranslations } from "next-intl"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  NetworkSpeedTestPanel,
  type NetworkSpeedTestPanelProps,
} from "@/components/layout/network-speed-test-panel"
import { useNetworkTraffic } from "@/hooks/use-network-traffic"
import { useNetworkSpeedTest } from "@/hooks/use-network-speed-test"
import {
  getTransport,
  getShellTransport,
  isDesktop,
  onTransportChange,
} from "@/lib/transport"

const serverTransport = () => null
const GOOD_LATENCY_LIMIT_MS = 200
const SLOW_LATENCY_LIMIT_MS = 500

function getLatencyColor(value: number | undefined, failed: boolean) {
  if (failed) return "text-red-700 dark:text-red-400"
  if (value === undefined) return "text-muted-foreground"
  if (value < GOOD_LATENCY_LIMIT_MS) return "text-green-700 dark:text-green-400"
  if (value < SLOW_LATENCY_LIMIT_MS) return "text-amber-700 dark:text-amber-400"
  return "text-red-700 dark:text-red-400"
}

export function NetworkStatusBar() {
  const [open, setOpen] = useState(false)
  const transport = useSyncExternalStore(
    onTransportChange,
    getTransport,
    serverTransport
  )
  const traffic = useNetworkTraffic(transport, open)
  const { test, start } = useNetworkSpeedTest(transport)
  const remote =
    transport && (transport !== getShellTransport() || !isDesktop())
  return (
    <div className="flex h-6 shrink-0 items-center text-xs text-muted-foreground">
      <NetworkStatusPopover
        {...traffic}
        test={test}
        onStart={() => void start()}
        open={open}
        onOpenChange={setOpen}
        remote={Boolean(remote)}
      />
    </div>
  )
}

function NetworkStatusPopover({
  open,
  onOpenChange,
  remote,
  ...props
}: NetworkSpeedTestPanelProps & {
  open: boolean
  onOpenChange: (open: boolean) => void
  remote: boolean
}) {
  const t = useTranslations("NetworkDiagnostics")
  const latency = props.test.outcomes.latency
  const latencyValue = latency?.result
    ? Math.round(latency.result.elapsed_ms)
    : undefined
  const label =
    latencyValue !== undefined
      ? `${latencyValue} ms`
      : t(latency?.error ? "failed" : "sampling")
  const color = getLatencyColor(latencyValue, Boolean(latency?.error))
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("open", {
            network: t(remote ? "serverNetwork" : "localNetwork"),
          })}
          aria-busy={props.test.phase !== null}
          className="flex h-6 shrink-0 items-center rounded-sm px-1 tabular-nums hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring"
        >
          <span className={color}>{label}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        side="bottom"
        align="end"
        aria-label={t("title")}
        className="max-h-[min(36rem,80dvh)] w-80 max-w-[calc(100vw-1.5rem)] overflow-y-auto rounded-lg"
      >
        <NetworkSpeedTestPanel {...props} />
      </PopoverContent>
    </Popover>
  )
}
