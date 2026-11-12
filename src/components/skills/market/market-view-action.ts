import { useEffect, useRef } from "react"

export interface MarketViewAction {
  id: number
  kind: "mine" | "upload" | "installed" | "market"
}

export function useMarketViewAction(
  action: MarketViewAction | null | undefined,
  apply: (kind: MarketViewAction["kind"]) => void
) {
  const handled = useRef<number | null>(null)
  const latest = useRef(apply)
  useEffect(() => {
    latest.current = apply
  })
  useEffect(() => {
    if (!action || handled.current === action.id) return
    let active = true
    queueMicrotask(() => {
      if (!active) return
      handled.current = action.id
      latest.current(action.kind)
    })
    return () => {
      active = false
    }
  }, [action])
}
