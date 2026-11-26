"use client"

import { useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2, RotateCcw } from "lucide-react"
import { useUiActions } from "@/components/intelligent-ui/actions"
import { useConversationPresentation } from "@/components/chat/conversation-presentation-context"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { TurnCompletion } from "@/lib/turn-completion"
import { flattenAssistantParts } from "./assistant-turn-timeline"

export type TurnPresentation = TurnCompletion | "unconfirmed" | "ended"

export function resolveTurnPresentation(
  parts: AdaptedContentPart[],
  completion?: TurnCompletion | null
): TurnPresentation {
  if (completion === "stopped" || completion === "interrupted")
    return completion
  const flat = flattenAssistantParts(parts).filter(
    (part) => part.type !== "text" || part.text.trim().length > 0
  )
  const hasResult = flat.some(
    (part) =>
      part.type === "displayed-image" ||
      part.type === "plugin-app" ||
      (part.type === "generated-image" && part.image != null)
  )
  // 过程中的说明不能证明任务已完成；最后应有正文或实际作品。
  if (!hasResult && flat[flat.length - 1]?.type !== "text") return "unconfirmed"
  return completion === "completed" ? "completed" : "ended"
}

function useTurnContinuation() {
  const continuation = useTranslations(
    "Folder.chat.acpConnections.autoContinuation"
  )
  const ask = useUiActions()
  const presentation = useConversationPresentation()
  const locked = useRef(false)
  const [pending, setPending] = useState(false)
  const [failed, setFailed] = useState(false)
  const busy =
    pending ||
    presentation?.status === "prompting" ||
    presentation?.status === "connecting" ||
    presentation?.awaitingInput
  const resume = async () => {
    if (!ask || locked.current || busy) return
    locked.current = true
    setPending(true)
    setFailed(false)
    try {
      await ask(continuation("continuePrompt"))
    } catch {
      setFailed(true)
    } finally {
      locked.current = false
      setPending(false)
    }
  }
  return {
    available: Boolean(ask),
    pending,
    failed,
    busy: Boolean(busy),
    resume,
  }
}

export function InterruptedTurnNotice({
  outcome,
  isLatest,
}: {
  outcome: Exclude<TurnPresentation, "completed" | "ended">
  isLatest: boolean
}) {
  const t = useTranslations("Folder.chat.messageList")
  const { available, pending, failed, busy, resume } = useTurnContinuation()
  const labels = {
    interrupted: "turnInterruptedHint",
    stopped: "turnStoppedHint",
    unconfirmed: "turnUnconfirmedHint",
  } as const
  const label = t(failed ? "turnContinueFailed" : labels[outcome])
  if (!isLatest || !available)
    return <p className="text-sm text-muted-foreground">{label}</p>
  return (
    <button
      type="button"
      onClick={() => void resume()}
      disabled={busy}
      className="flex min-h-9 items-center gap-1.5 text-left text-sm text-muted-foreground hover:text-foreground disabled:opacity-60"
      aria-busy={pending}
    >
      {pending ? (
        <Loader2 className="size-4 shrink-0 animate-spin" aria-hidden />
      ) : (
        <RotateCcw className="size-4 shrink-0" aria-hidden />
      )}
      <span>
        {label} {t(pending ? "turnContinuing" : "turnContinue")}
      </span>
    </button>
  )
}
