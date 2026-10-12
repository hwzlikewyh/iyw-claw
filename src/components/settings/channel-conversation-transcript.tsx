"use client"

import { useLayoutEffect, useMemo, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { ArrowDown, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { ContentPartsRenderer } from "@/components/message/content-parts-renderer"
import { BackgroundTaskStatusScope } from "@/components/message/background-task-status-context"
import { adaptMessageTurns } from "@/lib/adapters/ai-elements-adapter"
import type { AdaptedMessage } from "@/lib/adapters/ai-elements-adapter"
import { channelTranscript } from "@/lib/channel-transcript"
import type { ChannelConversationRuntime } from "@/lib/channel-runtime"
import type { DbConversationDetail, LiveSessionSnapshot } from "@/lib/types"

const BOTTOM_MARGIN_PX = 48
const PROCESS_PARTS = new Set([
  "reasoning",
  "tool-call",
  "tool-result",
  "tool-group",
  "agent-tool-call",
  "plan",
  "plan-mode",
  "delegation-status",
  "goal-tool-call",
  "goal-run",
  "background-task-group",
  "delegation-status-group",
  "collab-agent",
])

interface Props {
  data: {
    history: DbConversationDetail | null
    historyError: string | null
    live: LiveSessionSnapshot | null
    runtime: ChannelConversationRuntime
  } | null
  older: DbConversationDetail | null
  loading: boolean
  before: number
  loadingOlder: boolean
  onLoadOlder: () => Promise<void>
}

function useTranscriptMessages(props: Props) {
  const shared = useTranslations("Folder.chat.shared")
  const errors = useTranslations("Folder.chat.acpConnections.backendErrors")
  const transcript = useMemo(
    () =>
      props.data
        ? channelTranscript({
            history: props.data.history,
            live: props.data.live,
            older: props.older,
          })
        : { turns: [], streaming: new Set<number>() },
    [props.data, props.older]
  )
  const messages = useMemo(
    () =>
      adaptMessageTurns(
        transcript.turns,
        {
          attachedResources: shared("attachedResources"),
          toolCallFailed: shared("toolCallFailed"),
          runtimeErrors: {
            insufficientBalance: errors("insufficientBalance"),
            authenticationFailed: errors("authenticationFailed"),
            permissionDenied: errors("permissionDenied"),
            rateLimited: errors("rateLimited"),
            quotaExceeded: errors("quotaExceeded"),
            modelUnavailable: errors("modelUnavailable"),
            requestTimeout: errors("requestTimeout"),
            networkError: errors("networkError"),
            serviceUnavailable: errors("serviceUnavailable"),
            requestFailed: errors("requestFailed"),
          },
        },
        transcript.streaming
      ),
    [transcript, shared, errors]
  )
  return { transcript, messages }
}

function useTranscriptScroll(
  messages: AdaptedMessage[],
  loadingOlder: boolean
) {
  const scroll = useRef<HTMLDivElement>(null)
  const stick = useRef(true)
  const prepend = useRef<{ height: number; top: number } | null>(null)
  const [showLatest, setShowLatest] = useState(false)
  useLayoutEffect(() => {
    const element = scroll.current
    if (!element) return
    if (prepend.current && !loadingOlder) {
      element.scrollTop =
        prepend.current.top + element.scrollHeight - prepend.current.height
      prepend.current = null
    } else if (stick.current && !loadingOlder)
      element.scrollTop = element.scrollHeight
  }, [messages, loadingOlder])
  const onScroll = () => {
    const element = scroll.current
    if (!element) return
    stick.current =
      element.scrollHeight - element.scrollTop - element.clientHeight <
      BOTTOM_MARGIN_PX
    setShowLatest(!stick.current)
  }
  const rememberPosition = () => {
    if (scroll.current)
      prepend.current = {
        height: scroll.current.scrollHeight,
        top: scroll.current.scrollTop,
      }
  }
  const latest = () => {
    stick.current = true
    scroll.current?.scrollTo({
      top: scroll.current.scrollHeight,
      behavior: "smooth",
    })
  }
  return { scroll, onScroll, rememberPosition, showLatest, latest }
}

export function ChannelConversationTranscript(props: Props) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const { transcript, messages } = useTranscriptMessages(props)
  const { scroll, onScroll, rememberPosition, showLatest, latest } =
    useTranscriptScroll(messages, props.loadingOlder)
  const loadOlder = () => {
    rememberPosition()
    return props.onLoadOlder()
  }
  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scroll}
        className="h-full overflow-auto"
        role="log"
        aria-label={t("transcript")}
        onScroll={onScroll}
      >
        <OlderMessages
          before={props.before}
          loading={props.loadingOlder}
          onLoad={loadOlder}
        />
        <BackgroundTaskStatusScope
          turns={transcript.turns.map((turn) => ({
            key: turn.id,
            turn,
            phase: "persisted",
          }))}
        >
          {messages.map((message) => (
            <TranscriptMessage key={message.id} message={message} />
          ))}
        </BackgroundTaskStatusScope>
        {!messages.length && (
          <p role="status" className="p-6 text-sm text-muted-foreground">
            {t(
              props.loading
                ? "loading"
                : props.data?.historyError
                  ? "historyUnavailable"
                  : "noMessages"
            )}
          </p>
        )}
      </div>
      {showLatest && <LatestMessageButton onClick={latest} />}
    </div>
  )
}

function OlderMessages({
  before,
  loading,
  onLoad,
}: {
  before: number
  loading: boolean
  onLoad: () => Promise<void>
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  if (!before) return null
  return (
    <Button
      className="w-full"
      variant="ghost"
      disabled={loading}
      onClick={() => void onLoad()}
    >
      {loading && <Loader2 className="size-3.5 animate-spin" />}
      {t("olderMessages")}
    </Button>
  )
}

function LatestMessageButton({ onClick }: { onClick: () => void }) {
  const t = useTranslations("ChatChannelSettings.runtime")
  return (
    <Button
      size="sm"
      variant="secondary"
      className="absolute bottom-3 right-4 shadow"
      onClick={onClick}
    >
      <ArrowDown className="size-3.5" />
      {t("latest")}
    </Button>
  )
}
function TranscriptMessage({ message }: { message: AdaptedMessage }) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const content = message.content.filter(
    (part) => !PROCESS_PARTS.has(part.type)
  )
  const process = message.content.filter((part) => PROCESS_PARTS.has(part.type))
  return (
    <section
      className={`space-y-2 border-b px-4 py-3 ${message.role === "user" ? "bg-muted/30" : ""}`}
    >
      <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">
          {t(`roles.${message.role}`)}
        </span>
        <time dateTime={message.timestamp}>
          {new Date(message.timestamp).toLocaleString()}
        </time>
      </div>
      {content.length > 0 && (
        <ContentPartsRenderer parts={content} role={message.role} />
      )}
      {process.length > 0 && (
        <details>
          <summary className="cursor-pointer text-xs text-muted-foreground">
            {t("process")}
          </summary>
          <div className="mt-2">
            <ContentPartsRenderer parts={process} role={message.role} />
          </div>
        </details>
      )}
    </section>
  )
}
