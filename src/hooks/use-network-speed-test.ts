"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import {
  measureNetworkSpeed,
  NETWORK_PROBE_KINDS,
  type NetworkSpeedTest,
} from "@/lib/network-diagnostics"
import type { Transport } from "@/lib/transport"

const EMPTY_TEST: NetworkSpeedTest = {
  transport: null,
  phase: null,
  outcomes: {},
  completedAt: null,
}

interface TestRequest {
  transport: Transport
  cancelled: boolean
}

export function useNetworkSpeedTest(transport: Transport | null) {
  const scope = useMemo(() => ({ transport }), [transport])
  const [view, setView] = useState({ scope, test: EMPTY_TEST })
  const requestRef = useRef<TestRequest | null>(null)
  useEffect(() => {
    return () => {
      if (requestRef.current?.transport === transport) {
        requestRef.current.cancelled = true
        requestRef.current = null
      }
    }
  }, [transport])
  const start = useCallback(async () => {
    if (!transport || requestRef.current) return
    const request = { transport, cancelled: false }
    requestRef.current = request
    await runSpeedTest(request, (test) => setView({ scope, test }))
    if (!request.cancelled) requestRef.current = null
  }, [transport, scope])
  return { test: view.scope === scope ? view.test : EMPTY_TEST, start }
}

async function runSpeedTest(
  request: TestRequest,
  update: (test: NetworkSpeedTest) => void
) {
  let next: NetworkSpeedTest = {
    ...EMPTY_TEST,
    transport: request.transport,
    outcomes: {},
  }
  for (const kind of NETWORK_PROBE_KINDS) {
    if (request.cancelled) return
    next = { ...next, phase: kind }
    update(next)
    let outcome
    try {
      outcome = { result: await measureNetworkSpeed(request.transport, kind) }
    } catch (error) {
      outcome = { error: toErrorMessage(error) }
    }
    if (request.cancelled) return
    next = { ...next, outcomes: { ...next.outcomes, [kind]: outcome } }
    update(next)
  }
  update({ ...next, phase: null, completedAt: Date.now() })
}
