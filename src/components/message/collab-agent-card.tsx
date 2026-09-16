"use client"

import { useMemo } from "react"
import { AlertTriangle, Check, Clock3, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  parseCollabToolInput,
  classifyCollabStatus,
  isErrorCollabStatusKind,
  classifyCollabOp,
  shortAgentId,
  type CollabToolInfo,
} from "@/lib/collab-tool"
import { collabWaitLabel } from "@/lib/collab-presentation"
import { MessageResponse } from "@/components/ai-elements/message"
import { AgentCapsule } from "./agent-capsule"
import { CollabAgentRow } from "./collab-agent-row"
import type { ToolCallState } from "@/lib/adapters/ai-elements-adapter"

interface Props {
  input?: string | null
  output?: string | null
  errorText?: string | null
  state?: ToolCallState
}

export function CollabAgentCard({ input, output, errorText, state }: Props) {
  const t = useTranslations("Folder.chat.collabAgent")

  const info = useMemo(() => parseCollabToolInput(input), [input])
  const agents = info?.agents ?? []
  const op = info?.op ?? null
  const isWait = classifyCollabOp(op) === "wait"

  const hasErrorAgent = agents.some((a) =>
    isErrorCollabStatusKind(classifyCollabStatus(a.status))
  )
  const isError =
    state === "output-error" ||
    !!errorText?.trim() ||
    hasErrorAgent ||
    isErrorCollabStatusKind(classifyCollabStatus(info?.status ?? null))
  const isRunning = state === "input-streaming" || state === "input-available"

  const title =
    info?.prompt?.split("\n")[0]?.trim() || t(OP_LABEL[classifyCollabOp(op)])

  const idBadge =
    agents.length === 0
      ? null
      : agents.length > 1
        ? `${shortAgentId(agents[0].threadId)} +${agents.length - 1}`
        : shortAgentId(agents[0].threadId)

  return (
    <AgentCapsule
      title={title}
      isRunning={isRunning}
      isError={isError}
      rightSuffix={
        <CollabSuffix isError={isError} isWait={isWait} state={state} />
      }
      idBadge={idBadge}
      statusLabel={title}
      collapseOnComplete={false}
    >
      <CollabCardBody
        info={info}
        output={output}
        errorText={errorText}
        isRunning={isRunning}
        isError={isError}
      />
    </AgentCapsule>
  )
}

function CollabSuffix({
  isError,
  isWait,
  state,
}: {
  isError: boolean
  isWait: boolean
  state?: ToolCallState
}) {
  if (isError) return <AlertTriangle className="size-3.5 text-destructive" />
  if (isWait) return <Clock3 className="size-3.5" />
  return state === "output-available" ? <Check className="size-3.5" /> : null
}

const OP_LABEL = {
  spawn: "opSpawn",
  wait: "opWait",
  close: "opClose",
  resume: "opResume",
  other: "title",
} as const

function CollabCardBody({
  info,
  output,
  errorText,
  isRunning,
  isError,
}: {
  info: CollabToolInfo | null
  output?: string | null
  errorText?: string | null
  isRunning: boolean
  isError: boolean
}) {
  const tcp = useTranslations("Folder.chat.contentParts")
  const prompt = info?.prompt
  const agents = info?.agents ?? []
  const isWait = classifyCollabOp(info?.op ?? null) === "wait"
  return (
    <>
      {prompt && !agents.some((agent) => agent.task) && (
        <div className="space-y-1">
          <div className="text-xs font-medium text-muted-foreground">
            {tcp("agentPromptLabel")}
          </div>
          <div className="rounded-md bg-muted/50 p-3 text-xs text-muted-foreground prose prose-sm dark:prose-invert max-w-none [&_ul]:list-inside [&_ol]:list-inside">
            <MessageResponse>{prompt}</MessageResponse>
          </div>
        </div>
      )}

      {(isWait || agents.length === 0) && (
        <CollabWaitStatus
          isWait={isWait}
          isRunning={isRunning}
          isError={isError}
          output={output}
        />
      )}
      {agents.map((agent) => (
        <CollabAgentRow key={agent.threadId} agent={agent} />
      ))}
      {isError && errorText?.trim() && (
        <pre className="whitespace-pre-wrap break-words text-xs text-destructive">
          {errorText.trim()}
        </pre>
      )}
    </>
  )
}

function CollabWaitStatus({
  isWait,
  isRunning,
  isError,
  output,
}: {
  isWait: boolean
  isRunning: boolean
  isError: boolean
  output?: string | null
}) {
  const t = useTranslations("Folder.chat.collabAgent")
  let label = isWait ? collabWaitLabel(output) : ("noMessage" as const)
  if (isRunning) label = isWait ? "opWait" : "statusRunning"
  if (isError) label = "statusFailed"
  return (
    <div
      role="status"
      className="flex items-start gap-2 text-xs text-muted-foreground"
    >
      {isRunning && !isError ? (
        <Loader2 className="size-3.5 shrink-0 animate-spin" />
      ) : (
        <Clock3 className="size-3.5 shrink-0" />
      )}
      <span className="min-w-0 break-words">{t(label)}</span>
    </div>
  )
}
