import { useEffect, useState, type CSSProperties, type ReactNode } from "react"

const FOLD_DURATION_MS = 360

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
  useEffect(() => {
    const timer = window.setTimeout(
      () => setRetained(open),
      open ? 0 : FOLD_DURATION_MS
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
          "--assistant-fold-duration": `${FOLD_DURATION_MS}ms`,
        } as CSSProperties
      }
    >
      <div className="assistant-fold-row-inner">
        {(open || retained) && children}
      </div>
    </div>
  )
}
