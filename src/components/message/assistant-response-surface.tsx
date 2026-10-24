"use client"

import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { ChevronDown } from "lucide-react"
import { useTranslations } from "next-intl"
import { fromMarkdown } from "mdast-util-from-markdown"
import { MessageResponse } from "@/components/ai-elements/message"
import { useContentReading } from "./use-content-reading"
import { useStickToBottomContext } from "use-stick-to-bottom"
import { LIVE_TEXT_MAX_HEIGHT } from "./process-presentation"
import "./conversation-presentation.css"

const LONG_RESPONSE_LENGTH = 900
const SUMMARY_MAX_LENGTH = 600

interface AssistantResponseSurfaceProps {
  text: string
  collapse: boolean
  animateCompletion: boolean
  initiallyExpanded?: boolean
  children: ReactNode
}

function responsePreview(text: string): string | null {
  if (text.length < LONG_RESPONSE_LENGTH) return null
  const nodes = fromMarkdown(text).children
  // 只在完整的顶层段落后截取，不拆代码、列表、表格或交互组件。
  for (const node of nodes.slice(0, 2)) {
    const end = node.position?.end.offset
    if (node.type === "paragraph" && end && end <= SUMMARY_MAX_LENGTH) {
      const preview = text.slice(0, end)
      if (!preview.includes("<") && !preview.includes(":::")) return preview
    }
  }
  const heading = nodes[0]
  const end = heading?.position?.end.offset
  return heading?.type === "heading" && end ? text.slice(0, end) : ""
}

function useResponseDisclosure(
  animateCompletion: boolean,
  initiallyExpanded: boolean
) {
  const [manualOpen, setManualOpen] = useState<boolean | null>(
    initiallyExpanded ? true : null
  )
  const [autoOpen, setAutoOpen] = useState(animateCompletion)
  const contentRef = useRef<HTMLDivElement>(null)
  const reading = useContentReading(contentRef)
  const { escapedFromLock } = useStickToBottomContext()
  useEffect(() => {
    if (!animateCompletion || reading || escapedFromLock) return
    const timer = window.setTimeout(() => setAutoOpen(false), 120)
    return () => window.clearTimeout(timer)
  }, [animateCompletion, reading, escapedFromLock])
  return { contentRef, manualOpen, setManualOpen, autoOpen }
}

function ResponsePreview({
  preview,
  hidden,
}: {
  preview: string
  hidden: boolean
}) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div
      className="assistant-fold-row"
      data-hidden={hidden}
      inert={hidden}
      aria-hidden={hidden}
    >
      <div className="assistant-fold-row-inner">
        {preview ? (
          <MessageResponse>{preview}</MessageResponse>
        ) : (
          <p className="text-sm text-muted-foreground">{t("responseReady")}</p>
        )}
      </div>
    </div>
  )
}

function ResponseToggle({
  open,
  contentId,
  onToggle,
}: {
  open: boolean
  contentId: string
  onToggle: () => void
}) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <button
      type="button"
      className="assistant-response-toggle assistant-text-action"
      aria-expanded={open}
      aria-controls={contentId}
      onClick={onToggle}
    >
      <ChevronDown
        aria-hidden="true"
        className={`size-3.5 transition-transform duration-300 motion-reduce:transition-none ${open ? "rotate-180" : ""}`}
      />
      {t(open ? "collapseResponse" : "expandResponse")}
    </button>
  )
}

export function AssistantResponseSurface({
  text,
  collapse,
  animateCompletion,
  initiallyExpanded = false,
  children,
}: AssistantResponseSurfaceProps) {
  const contentId = useId()
  const { contentRef, manualOpen, setManualOpen, autoOpen } =
    useResponseDisclosure(animateCompletion, initiallyExpanded)
  const preview = useMemo(() => responsePreview(text), [text])
  const canFold = collapse && preview !== null
  const open = !canFold || (manualOpen ?? autoOpen)
  const [retained, setRetained] = useState(open)
  useEffect(() => {
    const reduceMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)"
    ).matches
    const timer = window.setTimeout(
      () => setRetained(open),
      open || reduceMotion ? 0 : 180
    )
    return () => window.clearTimeout(timer)
  }, [open])
  return (
    <div className="assistant-turn-summary">
      {canFold && <ResponsePreview preview={preview ?? ""} hidden={open} />}
      <div
        ref={contentRef}
        id={contentId}
        className="assistant-fold-row"
        data-hidden={!open}
        inert={!open}
        aria-hidden={!open}
      >
        <div
          className="assistant-fold-row-inner"
          style={
            canFold && animateCompletion && manualOpen === null
              ? { maxHeight: LIVE_TEXT_MAX_HEIGHT }
              : undefined
          }
        >
          {(open || retained) && children}
        </div>
      </div>
      {canFold && (
        <ResponseToggle
          open={open}
          contentId={contentId}
          onToggle={() => setManualOpen(!open)}
        />
      )}
    </div>
  )
}
