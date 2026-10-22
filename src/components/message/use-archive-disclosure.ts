import { useEffect, useRef, useState } from "react"
import { useStickToBottomContext } from "use-stick-to-bottom"
import { useContentReading } from "./use-content-reading"

const ARCHIVE_SETTLE_MS = 650

export function useArchiveDisclosure(archive: boolean) {
  const rootRef = useRef<HTMLDivElement>(null)
  const localReading = useContentReading(rootRef)
  const { escapedFromLock } = useStickToBottomContext()
  const reading = localReading || escapedFromLock
  const [autoArchived, setAutoArchived] = useState(archive)
  const [choice, setChoice] = useState<boolean | null>(null)
  useEffect(() => {
    if (reading) return
    const timer = window.setTimeout(
      () => setAutoArchived(archive),
      ARCHIVE_SETTLE_MS
    )
    return () => window.clearTimeout(timer)
  }, [archive, reading])
  return {
    rootRef,
    reading,
    hasChoice: choice !== null,
    open: choice ?? (!archive || !autoArchived),
    onOpenChange: setChoice,
  }
}
