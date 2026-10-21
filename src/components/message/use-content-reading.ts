import { useEffect, useState, type RefObject } from "react"
import { isNearScrollEnd } from "./use-process-auto-follow"

export function useContentReading(ref: RefObject<HTMLElement | null>) {
  const [reading, setReading] = useState(false)
  useEffect(() => {
    const root = ref.current
    if (!root) return
    const scrolled = new Set<HTMLElement>()
    const update = () => {
      for (const element of scrolled) {
        if (!root.contains(element) || isNearScrollEnd(element))
          scrolled.delete(element)
      }
      const selection = document.getSelection()
      const selecting = Boolean(
        selection &&
        !selection.isCollapsed &&
        (root?.contains(selection.anchorNode) ||
          root?.contains(selection.focusNode))
      )
      setReading(
        selecting ||
          Boolean(root.contains(document.activeElement)) ||
          scrolled.size > 0
      )
    }
    const onScroll = (event: Event) => {
      const element = event.target
      if (!(element instanceof HTMLElement)) return
      if (isNearScrollEnd(element)) scrolled.delete(element)
      else scrolled.add(element)
      update()
    }
    root.addEventListener("scroll", onScroll, true)
    document.addEventListener("selectionchange", update)
    document.addEventListener("focusin", update)
    document.addEventListener("focusout", update)
    return () => {
      root.removeEventListener("scroll", onScroll, true)
      document.removeEventListener("selectionchange", update)
      document.removeEventListener("focusin", update)
      document.removeEventListener("focusout", update)
    }
  }, [ref])
  return reading
}
