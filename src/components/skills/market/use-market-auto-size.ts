import { useEffect, useRef } from "react"
import { fitMarketPage } from "@/lib/market-pagination"

interface AutoSizeOptions {
  automatic?: boolean
  size: number
  onChange?: (size: number) => void
}
interface Measurement {
  width: number
  height: number
}

function measureCapacity(
  node: HTMLElement,
  scroll: HTMLElement,
  state: Measurement
): number | null {
  const grid =
    node.parentElement?.querySelector<HTMLElement>("[data-market-grid]")
  if (!grid?.getClientRects().length || !grid.clientWidth) return null
  const card =
    grid.querySelector<HTMLElement>("article") ??
    grid.querySelector<HTMLElement>("[data-market-skeleton]")
  if (!card) return null
  const width = card.getBoundingClientRect().width
  if (Math.abs(width - state.width) > 1) {
    state.width = width
    state.height = 0
  }
  if (card.tagName === "ARTICLE")
    state.height = Math.max(
      state.height,
      ...Array.from(
        grid.querySelectorAll("article"),
        (item) => item.getBoundingClientRect().height
      )
    )
  const style = getComputedStyle(grid)
  const top =
    grid.getBoundingClientRect().top -
    scroll.getBoundingClientRect().top +
    scroll.scrollTop
  const reserved =
    node.getBoundingClientRect().height +
    parseFloat(getComputedStyle(node).marginTop) +
    parseFloat(getComputedStyle(scroll).paddingBottom)
  return fitMarketPage({
    availableHeight: scroll.clientHeight - top - reserved,
    cardHeight: state.height || card.getBoundingClientRect().height,
    rowGap: parseFloat(style.rowGap) || 0,
    columns: style.gridTemplateColumns.split(" ").filter(Boolean).length,
  })
}

export function useMarketAutoSize(options: AutoSizeOptions) {
  const footer = useRef<HTMLElement>(null)
  const latest = useRef(options)
  const measurement = useRef<Measurement>({ width: 0, height: 0 })
  useEffect(() => {
    latest.current = options
  })
  useEffect(() => {
    if (!options.automatic || !options.onChange) return
    const node = footer.current
    const scroll = node?.closest<HTMLElement>("[data-market-scroll]")
    const parent = node?.parentElement
    if (!node || !scroll || !parent) return
    let frame = 0
    const measure = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        const size = measureCapacity(node, scroll, measurement.current)
        if (size == null || size === latest.current.size) return
        latest.current = { ...latest.current, size }
        latest.current.onChange?.(size)
      })
    }
    const resize = new ResizeObserver(measure)
    for (const element of [scroll, parent, node]) resize.observe(element)
    const mutation = new MutationObserver(measure)
    mutation.observe(parent, { childList: true, subtree: true })
    window.addEventListener("resize", measure)
    measure()
    return () => {
      cancelAnimationFrame(frame)
      resize.disconnect()
      mutation.disconnect()
      window.removeEventListener("resize", measure)
    }
  }, [options.automatic, options.onChange])
  return footer
}
