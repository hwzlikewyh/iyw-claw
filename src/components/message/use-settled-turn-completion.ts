import { useEffect, useRef, useState } from "react"
import { useStickToBottomContext } from "use-stick-to-bottom"
import { useContentReading } from "./use-content-reading"

const COMPLETE_SETTLE_MS = 650

export function useSettledTurnCompletion(complete: boolean) {
  const contentRef = useRef<HTMLDivElement>(null)
  const reading = useContentReading(contentRef)
  const { escapedFromLock } = useStickToBottomContext()
  const [settled, setSettled] = useState(complete)
  const [wasStreaming] = useState(!complete)
  useEffect(() => {
    if (complete && (reading || escapedFromLock)) return
    const timer = window.setTimeout(
      () => setSettled(complete),
      complete ? COMPLETE_SETTLE_MS : 0
    )
    return () => window.clearTimeout(timer)
  }, [complete, reading, escapedFromLock])
  return { contentRef, complete: complete && settled, wasStreaming }
}
