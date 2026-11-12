import { useEffect, useRef } from "react"

interface PageFocusOptions {
  page: number
  loading: boolean
  error?: string | null
  enabled?: boolean
}

export function useMarketPageFocus(options: PageFocusOptions) {
  const root = useRef<HTMLDivElement>(null)
  const previous = useRef(options.page)
  useEffect(() => {
    if (
      options.loading ||
      options.enabled === false ||
      previous.current === options.page
    )
      return
    const node = root.current
    previous.current = options.page
    if (!node?.getClientRects().length) return
    const focused = document.activeElement
    if (
      focused instanceof HTMLElement &&
      node.contains(focused) &&
      focused.matches("input,select,textarea,[role=combobox]")
    )
      return
    node.closest("[data-market-scroll]")?.scrollTo({ top: 0 })
    const target = options.error
      ? node.querySelector<HTMLElement>('[role="alert"]')
      : (node.querySelector<HTMLElement>("[data-market-card-title]") ??
        node.querySelector<HTMLElement>("[data-market-result-state]") ??
        node.querySelector<HTMLElement>('[role="status"]'))
    const next = target ?? node
    if (next.tabIndex < 0) next.tabIndex = -1
    next.focus({ preventScroll: true })
  }, [options.page, options.loading, options.error, options.enabled])
  return root
}
