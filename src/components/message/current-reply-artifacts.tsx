"use client"

import { memo, useMemo } from "react"

import { useTaskArtifacts } from "@/components/layout/use-task-artifacts"
import { CurrentReplyArtifactsPanel } from "@/components/message/current-reply-artifacts-panel"
import {
  extractAcceptedIds,
  extractAcceptedPaths,
  extractDeliveryArtifact,
  extractMessageId,
  indicatesRejectedArtifactCall,
  parseNestedRecord,
  parseRecord,
} from "@/components/message/current-reply-artifact-result"
import { useActiveFolder } from "@/contexts/active-folder-context"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { TaskArtifactInfo } from "@/lib/api"
import {
  isAbsoluteFilePath,
  normalizeSlashPath,
  toAbsoluteFilePath,
} from "@/lib/file-path-display"

interface CurrentReplyArtifactsProps {
  conversationId: number | null
  messageId: string
  parts: AdaptedContentPart[]
}

export interface ArtifactRegistration {
  hasCall: boolean
  rejected: boolean
  references: string[]
  artifactIds?: number[]
  messageId?: string
}

interface ArtifactToolCall {
  toolName: string
  input: string | null
  output?: string | null
}

const PRESENT_TASK_FILES_SUFFIX = /present[_-]task[_-]files$/i
const INVOKE_IYW_CAPABILITY_SUFFIX = /invoke[_-]iyw[_-]capability$/i
const IYW_IMAGE_SUFFIX = /generate[_-]iyw[_-]image$/i
const PRESENT_TASK_FILES_CAPABILITY_ID = "iyw.artifacts.present.v1"

export const CurrentReplyArtifacts = memo(function CurrentReplyArtifacts({
  conversationId,
  messageId,
  parts,
}: CurrentReplyArtifactsProps) {
  const registration = useMemo(
    () => extractArtifactRegistration(parts),
    [parts]
  )

  return (
    <ResolvedReplyArtifacts
      conversationId={conversationId}
      messageId={messageId}
      registration={registration}
    />
  )
})

function ResolvedReplyArtifacts({
  conversationId,
  messageId,
  registration,
}: {
  conversationId: number | null
  messageId: string
  registration: ArtifactRegistration
}) {
  const { activeFolder } = useActiveFolder()
  const query = useTaskArtifacts({
    conversationId,
    messageId: registration.messageId ?? messageId,
    folderId: null,
    scope: "current",
    latestTurnOnly: false,
    loadAll: true,
  })
  const items = useMemo(
    () => resolveReplyItems(registration, query.items, activeFolder?.path),
    [activeFolder?.path, query.items, registration]
  )

  if (items.length === 0) return null

  return <CurrentReplyArtifactsPanel items={items} />
}

export function extractArtifactRegistration(
  parts: AdaptedContentPart[]
): ArtifactRegistration {
  const calls: ArtifactToolCall[] = []
  collectArtifactToolCalls(parts, calls)
  const references: string[] = []
  const artifactIds: number[] = []
  let messageId: string | null = null
  let rejected = false
  for (const call of calls) {
    if (call.output && indicatesRejectedArtifactCall(call.output)) {
      rejected = true
      continue
    }
    messageId ??= extractMessageId(call.output)
    const accepted = extractAcceptedPaths(call.output)
    if (accepted?.length === 0) {
      rejected = true
      continue
    }
    references.push(...(accepted ?? extractInputPaths(call.input)))
    artifactIds.push(...extractAcceptedIds(call.output))
  }
  return {
    hasCall: calls.length > 0,
    rejected,
    references: dedupeStrings(references),
    ...(artifactIds.length ? { artifactIds: [...new Set(artifactIds)] } : {}),
    ...(messageId ? { messageId } : {}),
  }
}

function collectArtifactToolCalls(
  parts: AdaptedContentPart[],
  calls: ArtifactToolCall[]
): void {
  for (const part of parts) {
    if (part.type === "tool-call" && isArtifactToolCall(part)) {
      calls.push({
        toolName: part.toolName,
        input: part.input,
        output: part.output,
      })
    } else if (
      part.type === "generated-image" &&
      part.sourceToolName &&
      part.sourceToolOutput &&
      isArtifactToolCall({
        toolName: part.sourceToolName,
        input: null,
        output: part.sourceToolOutput,
      })
    ) {
      calls.push({
        toolName: part.sourceToolName,
        input: null,
        output: part.sourceToolOutput,
      })
    } else if (part.type === "tool-group") {
      collectArtifactToolCalls(part.items, calls)
    } else if (part.type === "goal-run") {
      collectArtifactToolCalls([part.start, ...part.items], calls)
      if (part.end) collectArtifactToolCalls([part.end], calls)
    }
  }
}

function isArtifactToolCall(call: ArtifactToolCall) {
  const toolName = call.toolName.trim()
  const input = parseRecord(call.input)
  const argumentsValue =
    parseNestedRecord(input?.arguments) ??
    parseNestedRecord(input?.input) ??
    input
  if (PRESENT_TASK_FILES_SUFFIX.test(toolName))
    return isPresentAction(argumentsValue)
  if (IYW_IMAGE_SUFFIX.test(toolName))
    return extractDeliveryArtifact(call.output) !== null
  if (!INVOKE_IYW_CAPABILITY_SUFFIX.test(toolName)) return false
  if (!isPresentAction(argumentsValue)) return false
  if (input?.capability_id === PRESENT_TASK_FILES_CAPABILITY_ID) return true
  const nestedName = input?.tool_name ?? input?.toolName
  return (
    typeof nestedName === "string" &&
    PRESENT_TASK_FILES_SUFFIX.test(nestedName.trim())
  )
}

function isPresentAction(input: Record<string, unknown> | null) {
  return input?.action === undefined || input.action === "present"
}

function extractInputPaths(input: string | null): string[] {
  const parsed = parseRecord(input)
  if (!parsed) return []
  const argumentsValue = parseNestedRecord(parsed.arguments)
  const inputValue = parseNestedRecord(parsed.input)
  return stringArray(parsed.files ?? argumentsValue?.files ?? inputValue?.files)
}

function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return value.filter((item): item is string => typeof item === "string")
}

function dedupeStrings(values: string[]): string[] {
  const seen = new Set<string>()
  return values.filter((value) => {
    const trimmed = value.trim()
    if (!trimmed || seen.has(trimmed)) return false
    seen.add(trimmed)
    return true
  })
}

function matchReplyArtifacts(
  references: string[],
  artifacts: TaskArtifactInfo[],
  folderPath?: string
): TaskArtifactInfo[] {
  const byPath = new Map(
    artifacts.map((artifact) => [artifactPathKey(artifact.path), artifact])
  )
  const matched = new Map<number, TaskArtifactInfo>()
  for (const reference of references) {
    for (const key of referencePathKeys(reference, folderPath)) {
      const artifact = byPath.get(key)
      if (artifact) matched.set(artifact.id, artifact)
    }
  }
  return Array.from(matched.values())
}

function resolveReplyItems(
  registration: ArtifactRegistration,
  items: TaskArtifactInfo[],
  folderPath?: string
): TaskArtifactInfo[] {
  if (registration.rejected) return []
  if (!registration.hasCall) return items
  if (registration.references.length === 0) return []
  if (registration.artifactIds?.length) {
    const ids = new Set(registration.artifactIds)
    return items.filter((item) => ids.has(item.id))
  }
  if (registration.messageId) {
    return items.filter((item) => item.messageId === registration.messageId)
  }
  return matchReplyArtifacts(registration.references, items, folderPath)
}

function referencePathKeys(reference: string, folderPath?: string): string[] {
  const trimmed = reference.trim()
  if (!trimmed) return []
  if (isHttpUrl(trimmed) || isAbsoluteFilePath(trimmed)) {
    return [artifactPathKey(trimmed)]
  }
  const absolute = toAbsoluteFilePath(trimmed, folderPath)
  return absolute ? [artifactPathKey(absolute)] : []
}

function artifactPathKey(path: string): string {
  if (isHttpUrl(path)) {
    try {
      return new URL(path).toString()
    } catch {
      return path.trim()
    }
  }
  const normalized = normalizeSlashPath(path).replace(/\/$/, "")
  return /^[a-z]:\//i.test(normalized) || normalized.startsWith("//")
    ? normalized.toLowerCase()
    : normalized
}

function isHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value)
}
