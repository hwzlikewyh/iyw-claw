"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"
import type { ToolCallInfo } from "@/contexts/acp-connections-context"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { activityAge, activityTimings } from "@/lib/session-activity"
import { useVisibleNow } from "@/hooks/use-visible-now"
import {
  inferLiveToolName,
  normalizeToolName,
} from "@/lib/tool-call-normalization"
import {
  getBuiltinToolDisplay,
  getRemoteToolDisplayTitle,
} from "@/lib/builtin-tool-display"
import {
  getReadableToolActivityTitle,
  getToolDisplayName,
} from "@/lib/tool-display"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import { isContextCompactionMeta } from "@/lib/context-compaction"
import { readableStatusText, summarizeLiveTurn } from "./live-turn-summary"

export type ActivityIcon =
  | "image"
  | "command"
  | "search"
  | "file"
  | "memory"
  | "browser"
  | "task"
  | "tool"
  | "thinking"
  | "reply"
  | "wait"
  | "retry"
  | "input"

type TurnSummary = ReturnType<typeof summarizeLiveTurn>
type SessionActivity = ReturnType<typeof useSessionActivity>

const MILLISECONDS_PER_SECOND = 1_000
const WAITING_PHASES = new Set([
  "awaitingUser",
  "pendingTool",
  "waitingOutput",
  "activityUnknown",
  "waitingProcess",
])
interface ActivityInput {
  summary: TurnSummary
  awaitingUser?: boolean
  visible?: boolean
  startedAt?: number | null
}

function selectPhase(
  summary: TurnSummary,
  activity: SessionActivity,
  now: number
) {
  if (activity?.snapshot.retrying_since) return "runtimeRetrying"
  if (summary.activeTool)
    return summary.activeTool.status === "pending"
      ? "pendingTool"
      : "runningTool"
  const { process } = activityTimings(activity, {
    now,
    hasActiveTool: false,
  })
  if (process) return "waitingProcess"
  // 以真实内容和工具边界推进阶段，不用分片间隔推断工作停止。
  const latest = summary.latestKind ?? summary.lastContent
  const { textAge, thinkingAge, toolAge } = observedEventAges(activity, now)
  if (
    textAge !== null &&
    (toolAge === null || textAge < toolAge) &&
    (thinkingAge === null || textAge <= thinkingAge)
  )
    return "streaming"
  if (
    thinkingAge !== null &&
    (toolAge === null || thinkingAge < toolAge) &&
    (textAge === null || thinkingAge < textAge)
  )
    return "thinking"
  if (
    toolAge !== null &&
    (textAge === null || toolAge < textAge) &&
    (thinkingAge === null || toolAge < thinkingAge)
  )
    return "waitingOutput"
  if (latest === "thinking" || latest === "plan") return "thinking"
  if (latest === "text") return "streaming"
  if (latest === "tool_call") return "waitingOutput"
  return activity ? "waitingOutput" : "activityUnknown"
}

function observedEventAges(activity: SessionActivity, now: number) {
  const snapshot = activity?.snapshot
  const output = activityAge(activity, snapshot?.tool_output_at, now)
  const start = activityAge(activity, snapshot?.tool_started_at, now)
  return {
    textAge: activityAge(activity, snapshot?.text_at, now),
    thinkingAge: activityAge(activity, snapshot?.thinking_at, now),
    toolAge:
      output == null ? start : start == null ? output : Math.min(start, output),
  }
}

function toolIcon(name: string): ActivityIcon {
  if (/image|picture/.test(name)) return "image"
  if (/browser|webfetch/.test(name)) return "browser"
  if (/memory/.test(name)) return "memory"
  if (/search|grep|glob|find|query/.test(name)) return "search"
  if (/bash|exec_command|shell/.test(name)) return "command"
  if (/read|write|edit|patch/.test(name)) return "file"
  if (/agent|task|plan|goal/.test(name)) return "task"
  return "tool"
}

const PHASE_ICONS: Record<string, ActivityIcon> = {
  awaitingUser: "input",
  runtimeRetrying: "retry",
  waitingProcess: "command",
  streaming: "reply",
  thinking: "thinking",
  waitingOutput: "wait",
  activityUnknown: "wait",
}

function resolvedToolTitle(
  toolCallId: string | undefined,
  parts: readonly AdaptedContentPart[] | undefined
): string | null {
  if (!toolCallId) return null
  for (const part of parts ?? []) {
    if (part.type === "tool-call" && part.toolCallId === toolCallId)
      return part.displayTitle ?? null
    if (part.type !== "tool-group" && part.type !== "goal-run") continue
    const title = resolvedToolTitle(toolCallId, part.items)
    if (title) return title
  }
  return null
}

function useToolPresentation(
  tool: ToolCallInfo | null,
  parts: readonly AdaptedContentPart[] | undefined
) {
  const t = useTranslations("Folder.chat.contentParts")
  const toolCallId = tool?.tool_call_id
  const resolvedTitle = useMemo(
    () => resolvedToolTitle(toolCallId, parts),
    [toolCallId, parts]
  )
  const title = tool?.title
  const kind = tool?.kind
  const rawInput = tool?.raw_input
  const meta = tool?.meta
  // 输出块变化不重新解析工具输入和名称。
  return useMemo(() => {
    if (isContextCompactionMeta(meta))
      return {
        name: t("contextCompaction.action"),
        icon: "memory" as ActivityIcon,
      }
    if (!title && !kind) return { name: "", icon: "tool" as ActivityIcon }
    const name = inferLiveToolName({ title, kind, rawInput, meta })
    const builtin = getBuiltinToolDisplay(name, rawInput, resolvedTitle)
    const builtinTitle = getReadableToolActivityTitle(builtin?.fallbackName)
    const activityTitle = getReadableToolActivityTitle(resolvedTitle ?? title)
    return {
      name:
        builtinTitle ??
        activityTitle ??
        getToolDisplayName(
          {
            toolName: builtin?.toolName ?? name,
            displayTitle:
              getRemoteToolDisplayTitle(rawInput, meta) ?? undefined,
          },
          (key, values) =>
            t.has(key as never) ? t(key as never, values as never) : null
        ),
      icon: toolIcon(builtin?.toolName ?? normalizeToolName(name)),
    }
  }, [title, kind, rawInput, meta, resolvedTitle, t])
}

export function useTurnActivity(
  contextKey: string | undefined,
  input: ActivityInput & { parts?: readonly AdaptedContentPart[] }
) {
  const activity = useSessionActivity(contextKey)
  const t = useTranslations("Folder.chat.liveTurnStats")
  const clock = useVisibleNow(!input.awaitingUser && input.visible !== false)
  const now = Math.max(clock, activity?.receivedAt ?? 0)
  const phase = input.awaitingUser
    ? "awaitingUser"
    : selectPhase(input.summary, activity, now)
  const tool = useToolPresentation(
    input.summary.activeTool ?? input.summary.latestTool ?? null,
    input.parts
  )
  const feedback = useActivityFeedback(activity, {
    summary: input.summary,
    phase,
    tool: tool.name,
    now,
    startedAt: input.startedAt,
  })
  const retry = useRetryPresentation(activity, phase)
  const icon = PHASE_ICONS[phase] ?? tool.icon
  const detail = useStepDescription(
    input.summary,
    phase,
    retry.detail,
    tool.name
  )
  return {
    phase: retry.label ?? t(phase, { model: "原助理", tool: tool.name }),
    detail: feedback.detail ?? detail,
    elapsed: feedback.elapsed,
    icon,
    waiting: WAITING_PHASES.has(phase),
    attention: phase === "awaitingUser" || phase === "runtimeRetrying",
  }
}

function useStepDescription(
  summary: TurnSummary,
  phase: string,
  fallback: string | null,
  toolName: string
) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  if (phase === "waitingProcess" && toolName)
    return t("recentTool", { tool: toolName })
  const step = summary.planEntries.find(
    (entry) => entry.status === "in_progress"
  )
  const text = step ? readableStatusText(step.content) : null
  return text && (phase === "runningTool" || phase === "thinking")
    ? t("currentStep", { step: text })
    : (fallback ?? t(`detail.${phase}` as never))
}

function useActivityFeedback(
  activity: SessionActivity,
  input: {
    summary: TurnSummary
    phase: string
    tool: string
    now: number
    startedAt?: number | null
  }
) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const elapsed =
    activityAge(activity, activity?.snapshot.started_at, input.now) ??
    (input.startedAt == null ? null : Math.max(0, input.now - input.startedAt))
  const result = {
    elapsed:
      elapsed == null
        ? null
        : t("elapsed", { elapsed: formatElapsedLabel(elapsed, t) }),
    detail: null as string | null,
  }
  if (input.phase === "waitingOutput" && input.summary.latestTool && input.tool)
    result.detail = t("recentTool", { tool: input.tool })
  if (input.phase !== "runningTool" && input.phase !== "waitingProcess")
    return result
  const toolAge = activityAge(
    activity,
    activity?.snapshot.tool_started_at,
    input.now
  )
  const timing = activityTimings(activity, {
    now: input.now,
    hasActiveTool: true,
  })
  if (toolAge !== null && timing.silence !== null)
    result.detail = t("toolWaitProgress", {
      tool: input.tool,
      elapsed: Math.floor(toolAge / MILLISECONDS_PER_SECOND),
      silence: Math.floor(timing.silence / MILLISECONDS_PER_SECOND),
    })
  return result
}

function useRetryPresentation(activity: SessionActivity, phase: string) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const snapshot = activity?.snapshot
  if (phase !== "runtimeRetrying" || !snapshot)
    return { label: null, detail: null }
  const label =
    snapshot.retry_attempt && snapshot.retry_max_attempts
      ? t("runtimeRetryingWithAttempt", {
          attempt: snapshot.retry_attempt,
          max: snapshot.retry_max_attempts,
        })
      : null
  const known = [
    "streamDisconnected",
    "rateLimited",
    "serverError",
    "networkError",
  ]
  const detail =
    snapshot.retry_reason && known.includes(snapshot.retry_reason)
      ? t(`retryReason.${snapshot.retry_reason}` as never)
      : null
  return { label, detail }
}
