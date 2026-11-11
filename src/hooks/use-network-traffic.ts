"use client"

import { useEffect, useMemo, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import {
  calculateNetworkRates,
  NETWORK_SAMPLE_INTERVAL_MS,
  readNetworkTraffic,
  type NetworkRates,
  type NetworkTrafficSnapshot,
} from "@/lib/network-diagnostics"
import type { Transport } from "@/lib/transport"

interface TrafficView {
  rates: NetworkRates | null
  error: string | null
  empty: boolean
}

const EMPTY_VIEW: TrafficView = {
  rates: null,
  error: null,
  empty: false,
}

export function useNetworkTraffic(transport: Transport | null) {
  const scope = useMemo(() => ({ transport }), [transport])
  const [view, setView] = useState({ scope, traffic: EMPTY_VIEW })
  useEffect(() => {
    if (!transport) return
    return pollTraffic(transport, (traffic) => setView({ scope, traffic }))
  }, [transport, scope])
  return view.scope === scope ? view.traffic : EMPTY_VIEW
}

function pollTraffic(
  transport: Transport,
  update: (view: TrafficView) => void
) {
  let stopped = false
  let previous: NetworkTrafficSnapshot | null = null
  let preferredName: string | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  const sample = async () => {
    if (document.visibilityState === "hidden") {
      previous = null
    } else {
      try {
        const snapshot = await readNetworkTraffic(transport)
        if (stopped) return
        const rates = calculateNetworkRates(previous, snapshot, preferredName)
        preferredName = rates?.name
        previous = snapshot
        update({ rates, error: null, empty: !snapshot.interfaces.length })
      } catch (error) {
        if (stopped) return
        previous = null
        update({ rates: null, error: toErrorMessage(error), empty: false })
      }
    }
    if (!stopped)
      timer = setTimeout(() => void sample(), NETWORK_SAMPLE_INTERVAL_MS)
  }
  void sample()
  return () => {
    stopped = true
    clearTimeout(timer)
  }
}
