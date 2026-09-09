"use client"

import { useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import type { ToolCallInfo } from "@/contexts/acp-connections-context"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { OUTPUT_RATE_WINDOW_MS } from "@/hooks/use-recent-output-rate"
import { activityTimings } from "@/lib/session-activity"
import {
  inferLiveToolName,
  normalizeToolName,
} from "@/lib/tool-call-normalization"
import { getBuiltinToolDisplay } from "@/lib/builtin-tool-display"
import { getToolDisplayName } from "@/lib/tool-display"
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

function useActivityNow(activity: SessionActivity, enabled: boolean) {
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    if (!enabled || !activity) return
    let timer: ReturnType<typeof setTimeout> | undefined
    const schedule = () => {
      clearTimeout(timer)
      if (document.hidden) return
      const timing = activityTimings(activity, {
        now: Date.now(),
        hasActiveTool: false,
      })
      const ages = [timing.textAge, timing.thinkingAge].filter(
        (age): age is number => age !== null && age < OUTPUT_RATE_WINDOW_MS
      )
      if (ages.length === 0) return
      const delay = OUTPUT_RATE_WINDOW_MS - Math.max(...ages)
      timer = setTimeout(() => setNow(Date.now()), delay)
    }
    const visible = () => {
      if (!document.hidden) setNow(Date.now())
      schedule()
    }
    schedule()
    document.addEventListener("visibilitychange", visible)
    return () => {
      clearTimeout(timer)
      document.removeEventListener("visibilitychange", visible)
    }
  }, [activity, enabled, now])
  return Math.max(now, activity?.receivedAt ?? 0)
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
  const { textAge, thinkingAge, process } = activityTimings(activity, {
    now,
    hasActiveTool: false,
  })
  if (process) return "waitingProcess"
  if (textAge !== null && textAge < OUTPUT_RATE_WINDOW_MS) return "streaming"
  if (thinkingAge !== null && thinkingAge < OUTPUT_RATE_WINDOW_MS)
    return "thinking"
  if (!activity && summary.lastContent)
    return summary.lastContent === "text" ? "streaming" : "thinking"
  return activity ? "waitingOutput" : "activityUnknown"
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

function useToolPresentation(tool: ToolCallInfo | null) {
  const t = useTranslations("Folder.chat.contentParts")
  const title = tool?.title
  const kind = tool?.kind
  const rawInput = tool?.raw_input
  const meta = tool?.meta
  // 输出块变化不重新解析工具输入和名称。
  return useMemo(() => {
    if (!title && !kind) return { name: "", icon: "tool" as ActivityIcon }
    const name = inferLiveToolName({ title, kind, rawInput, meta })
    const builtin = getBuiltinToolDisplay(name, rawInput)
    return {
      name: getToolDisplayName(
        { toolName: name, input: rawInput, displayTitle: title },
        (key) => (t.has(key as never) ? t(key as never) : null)
      ),
      icon: toolIcon(builtin?.toolName ?? normalizeToolName(name)),
    }
  }, [title, kind, rawInput, meta, t])
}

export function useTurnActivity(
  contextKey: string | undefined,
  input: { summary: TurnSummary; awaitingUser?: boolean }
) {
  const activity = useSessionActivity(contextKey)
  const t = useTranslations("Folder.chat.liveTurnStats")
  const enabled =
    !input.awaitingUser &&
    !input.summary.activeTool &&
    !activity?.snapshot.retrying_since
  const now = useActivityNow(activity, enabled)
  const phase = input.awaitingUser
    ? "awaitingUser"
    : selectPhase(input.summary, activity, now)
  const tool = useToolPresentation(input.summary.activeTool)
  const icon = PHASE_ICONS[phase] ?? tool.icon
  const step = input.summary.planEntries.find(
    (entry) => entry.status === "in_progress"
  )
  const stepText = step ? readableStatusText(step.content) : null
  const detail =
    stepText && (phase === "runningTool" || phase === "thinking")
      ? t("currentStep", { step: stepText })
      : t(`detail.${phase}` as never)
  return {
    phase: t(phase, { model: "原助理", tool: tool.name }),
    detail,
    icon,
    waiting: [
      "awaitingUser",
      "pendingTool",
      "waitingOutput",
      "activityUnknown",
      "waitingProcess",
    ].includes(phase),
    attention: phase === "awaitingUser" || phase === "runtimeRetrying",
  }
}
