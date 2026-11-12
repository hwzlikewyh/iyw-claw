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
  const latencyValue = latency?.result?.elapsed_ms
  const label =
    latencyValue !== undefined
      ? t("latencyValue", { value: Math.round(latencyValue) })
      : t(latency?.error ? "failed" : "latencySampling")
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("open", {
            network: t(remote ? "serverNetwork" : "localNetwork"),
          })}
          aria-busy={props.test.phase !== null}
          className="flex h-6 shrink-0 items-center gap-1.5 rounded-sm px-1 tabular-nums hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        >
          <span>{t(remote ? "serverNetwork" : "localNetwork")}</span>
          <span>{label}</span>
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
