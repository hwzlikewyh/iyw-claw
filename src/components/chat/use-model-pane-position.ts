"use client"

import { useLayoutEffect, useRef, useState, type CSSProperties } from "react"

const VIEWPORT_GAP = 8

function paneSide(anchor: HTMLElement, width: number) {
  const bounds = anchor.getBoundingClientRect()
  const rightFits = window.innerWidth - bounds.right >= width
  const leftFits = bounds.left >= width
  const parentPane = anchor.closest<HTMLElement>("[data-model-pane]")
  if (parentPane) {
    return parentPane.style.right === "100%"
      ? leftFits
        ? "left"
        : "inline"
      : rightFits
        ? "right"
        : "inline"
  }
  return rightFits ? "right" : leftFits ? "left" : "inline"
}

// 所有卡片留在同一个 Popover 内，避免 WebView 的嵌套弹层丢失点击。
export function useModelPanePosition(compact: boolean, offset = 0) {
  const paneRef = useRef<HTMLDivElement>(null)
  const [style, setStyle] = useState<CSSProperties>({
    position: "absolute",
    left: "100%",
    top: 0,
  })
  useLayoutEffect(() => {
    const pane = paneRef.current
    const anchor = pane?.parentElement
    if (!pane || !anchor) return
    const position = () => {
      const bounds = anchor.getBoundingClientRect()
      const width = pane.offsetWidth
      const side = paneSide(anchor, width)
      if (compact || side === "inline") {
        setStyle({ position: "static", marginTop: VIEWPORT_GAP })
        return
      }
      const top = Math.min(
        offset,
        window.innerHeight - VIEWPORT_GAP - bounds.top - pane.offsetHeight
      )
      setStyle({
        position: "absolute",
        top: Math.max(VIEWPORT_GAP - bounds.top, top),
        ...(side === "right" ? { left: "100%" } : { right: "100%" }),
      })
    }
    position()
    const observer = new ResizeObserver(position)
    observer.observe(pane)
    observer.observe(anchor)
    window.addEventListener("resize", position)
    return () => {
      observer.disconnect()
      window.removeEventListener("resize", position)
    }
  }, [compact, offset])
  return { paneRef, style }
}
