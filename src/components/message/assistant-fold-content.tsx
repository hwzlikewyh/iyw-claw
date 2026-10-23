import { useEffect, useState, type CSSProperties, type ReactNode } from "react"

const OPEN_DURATION_MS = 240
const CLOSE_DURATION_MS = 180

export function AssistantFoldContent({
  open,
  contentId,
  children,
}: {
  open: boolean
  contentId?: string
  children: ReactNode
}) {
  const [retained, setRetained] = useState(open)
  const duration = open ? OPEN_DURATION_MS : CLOSE_DURATION_MS
  useEffect(() => {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches
    const timer = window.setTimeout(
      () => setRetained(open),
      open || reduce ? 0 : CLOSE_DURATION_MS
    )
    return () => window.clearTimeout(timer)
  }, [open])
  return (
    <div
      id={contentId}
      className="assistant-fold-row"
      data-hidden={!open}
      inert={!open}
      aria-hidden={!open}
      style={
        {
          "--assistant-fold-duration": `${duration}ms`,
        } as CSSProperties
      }
    >
      <div className="assistant-fold-row-inner">
        {(open || retained) && children}
      </div>
    </div>
  )
}
