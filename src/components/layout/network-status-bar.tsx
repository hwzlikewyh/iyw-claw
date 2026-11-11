"use client"

import { useSyncExternalStore } from "react"
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
import { formatNetworkRate } from "@/lib/network-diagnostics"
import {
  getTransport,
  getShellTransport,
  isDesktop,
  onTransportChange,
} from "@/lib/transport"

const serverTransport = () => null

export function NetworkStatusBar() {
  const t = useTranslations("NetworkDiagnostics")
  const transport = useSyncExternalStore(
    onTransportChange,
    getTransport,
    serverTransport
  )
  const traffic = useNetworkTraffic(transport)
  const { test, start } = useNetworkSpeedTest(transport)
  const remote =
    transport && (transport !== getShellTransport() || !isDesktop())
  return (
    <div className="flex h-7 shrink-0 items-center justify-between gap-2 border-t border-border bg-muted px-3 text-xs text-muted-foreground">
      <span className="min-w-0 truncate">
        {t(remote ? "serverNetwork" : "localNetwork")}
      </span>
      <NetworkStatusPopover
        {...traffic}
        test={test}
        onStart={() => void start()}
      />
    </div>
  )
}

function NetworkStatusPopover(props: NetworkSpeedTestPanelProps) {
  const t = useTranslations("NetworkDiagnostics")
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("open")}
          className="flex h-full shrink-0 items-center gap-2 rounded-sm px-1 tabular-nums hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        >
          <span>
            {props.rates
              ? `↓ ${formatNetworkRate(props.rates.download)}  ↑ ${formatNetworkRate(props.rates.upload)} Mbps`
              : t(props.error || props.empty ? "unavailable" : "sampling")}
          </span>
          <span>{props.test.phase ? t("running") : t("test")}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="end"
        aria-label={t("title")}
        className="max-h-[min(36rem,80dvh)] w-80 max-w-[calc(100vw-1.5rem)] overflow-y-auto rounded-lg"
      >
        <NetworkSpeedTestPanel {...props} />
      </PopoverContent>
    </Popover>
  )
}
