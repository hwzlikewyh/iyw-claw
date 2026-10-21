import { useEffect, useRef, useState } from "react"
import { useStickToBottomContext } from "use-stick-to-bottom"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { processTailStart } from "./process-presentation"
import { useContentReading } from "./use-content-reading"

const ARCHIVE_SETTLE_MS = 650

export function useProcessArchive(
  parts: AdaptedContentPart[],
  complete: boolean,
  open: boolean
) {
  const rootRef = useRef<HTMLDivElement>(null)
  const localReading = useContentReading(rootRef)
  const { escapedFromLock } = useStickToBottomContext()
  const reading = localReading || escapedFromLock
  const [hiddenBefore, setHiddenBefore] = useState(complete ? parts.length : 0)
  const target = complete ? parts.length : processTailStart(parts)
  useEffect(() => {
    if (open || reading) return
    const timer = window.setTimeout(
      () => setHiddenBefore(target),
      ARCHIVE_SETTLE_MS
    )
    return () => window.clearTimeout(timer)
  }, [open, reading, target])
  return { rootRef, reading, hiddenBefore: open ? 0 : hiddenBefore }
}
