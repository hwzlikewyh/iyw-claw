"use client"

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react"

type AnchorKind = "activity" | "monitor" | "output"
interface AnchorReport {
  id: string
  kind: AnchorKind
  visible: boolean
}
interface VisibilityState {
  activityVisible: boolean
  monitorVisible: boolean
  outputVisible: boolean
  unreadProgress: boolean
  setUnreadProgress: (unread: boolean) => void
  outputStreaming: boolean
  setOutputStreaming: (streaming: boolean) => void
  pageVisible: boolean
  report: (report: AnchorReport) => void
}
const VisibilityContext = createContext<VisibilityState>({
  activityVisible: false,
  monitorVisible: false,
  outputVisible: false,
  unreadProgress: false,
  setUnreadProgress: () => {},
  outputStreaming: false,
  setOutputStreaming: () => {},
  pageVisible: true,
  report: () => {},
})

function subscribeVisibility(listener: () => void) {
  document.addEventListener("visibilitychange", listener)
  return () => document.removeEventListener("visibilitychange", listener)
}
const pageVisible = () => !document.hidden

function useVisibleAnchors() {
  const anchors = useRef(new Map<string, AnchorKind>())
  const [visible, setVisible] = useState({
    activityVisible: false,
    monitorVisible: false,
    outputVisible: false,
  })
  const report = useCallback(({ id, kind, visible }: AnchorReport) => {
    if (visible) anchors.current.set(id, kind)
    else anchors.current.delete(id)
    const entries = [...anchors.current.values()]
    const activityVisible = entries.some((entry) => entry === "activity")
    const monitorVisible = entries.some((entry) => entry === "monitor")
    const outputVisible = entries.some((entry) => entry === "output")
    setVisible((previous) =>
      previous.activityVisible === activityVisible &&
      previous.monitorVisible === monitorVisible &&
      previous.outputVisible === outputVisible
        ? previous
        : { activityVisible, monitorVisible, outputVisible }
    )
  }, [])
  return { ...visible, report }
}

export function ConversationActivityVisibility({
  children,
}: {
  children: ReactNode
}) {
  const visible = useVisibleAnchors()
  const [unreadProgress, setUnreadProgress] = useState(false)
  const [outputStreaming, setOutputStreaming] = useState(false)
  const shown = useSyncExternalStore(
    subscribeVisibility,
    pageVisible,
    () => true
  )
  return (
    <VisibilityContext.Provider
      value={{
        ...visible,
        pageVisible: shown,
        unreadProgress,
        setUnreadProgress,
        outputStreaming,
        setOutputStreaming,
      }}
    >
      <div
        className="conversation-presentation-root contents"
        data-page-visible={shown}
        data-activity-visible={visible.activityVisible}
        data-output-visible={visible.outputVisible}
        data-output-streaming={outputStreaming}
      >
        {children}
      </div>
    </VisibilityContext.Provider>
  )
}

export function useConversationActivityVisibility() {
  return useContext(VisibilityContext)
}

export function useConversationActivityAnchor(kind: AnchorKind) {
  const ref = useRef<HTMLDivElement>(null)
  const id = useId()
  const { report } = useConversationActivityVisibility()
  useEffect(() => {
    const node = ref.current
    if (!node || typeof IntersectionObserver === "undefined") return
    const observer = new IntersectionObserver(
      ([entry]) =>
        report({
          id,
          kind,
          visible: entry.isIntersecting && entry.intersectionRatio >= 0.5,
        }),
      { threshold: 0.5 }
    )
    observer.observe(node)
    return () => {
      observer.disconnect()
      report({ id, kind, visible: false })
    }
  }, [id, kind, report])
  return ref
}
