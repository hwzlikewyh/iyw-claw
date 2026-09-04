"use client"

import { memo, useMemo } from "react"

import { useTaskArtifacts } from "@/components/layout/use-task-artifacts"
import { CurrentReplyArtifactsPanel } from "@/components/message/current-reply-artifacts-panel"
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
  const items = useMemo(() => {
    if (registration.rejected) return []
    if (!registration.hasCall) return query.items
    if (registration.references.length === 0) return []
    return matchReplyArtifacts(
      registration.references,
      query.items,
      activeFolder?.path
    )
  }, [activeFolder?.path, query.items, registration])

  if (items.length === 0) return null

  return <CurrentReplyArtifactsPanel items={items} />
}

export function extractArtifactRegistration(
  parts: AdaptedContentPart[]
): ArtifactRegistration {
  const calls: ArtifactToolCall[] = []
  collectArtifactToolCalls(parts, calls)
  const references: string[] = []
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
  }
  return {
    hasCall: calls.length > 0,
    rejected,
    references: dedupeStrings(references),
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
  if (PRESENT_TASK_FILES_SUFFIX.test(toolName)) return true
  if (IYW_IMAGE_SUFFIX.test(toolName))
    return extractDeliveryArtifact(call.output) !== null
  if (!INVOKE_IYW_CAPABILITY_SUFFIX.test(toolName)) return false
  const input = parseRecord(call.input)
  if (input?.capability_id === PRESENT_TASK_FILES_CAPABILITY_ID) return true
  const nestedName = input?.tool_name ?? input?.toolName
  return (
    typeof nestedName === "string" &&
    PRESENT_TASK_FILES_SUFFIX.test(nestedName.trim())
  )
}

function extractInputPaths(input: string | null): string[] {
  const parsed = parseRecord(input)
  if (!parsed) return []
  const argumentsValue = parseNestedRecord(parsed.arguments)
  const inputValue = parseNestedRecord(parsed.input)
  return stringArray(parsed.files ?? argumentsValue?.files ?? inputValue?.files)
}

function extractAcceptedPaths(
  output: string | null | undefined
): string[] | null {
  if (output && indicatesRejectedArtifactCall(output)) return []
  const parsed = parseRecord(output)
  if (!parsed) return null
  const result = parseNestedRecord(parsed.result)
  const structured = parseNestedRecord(
    parsed.structuredContent ?? parsed.structured_content
  )
  const resultStructured = parseNestedRecord(
    result?.structuredContent ?? result?.structured_content
  )
  const delivery = extractDeliveryArtifact(output)
  const accepted =
    structured?.accepted ??
    parsed.accepted ??
    resultStructured?.accepted ??
    result?.accepted ??
    delivery?.accepted
  return Array.isArray(accepted) ? acceptedPaths(accepted) : null
}

function extractMessageId(output: string | null | undefined): string | null {
  const parsed = parseRecord(output)
  if (!parsed) return null
  const result = parseNestedRecord(parsed.result)
  const structured = parseNestedRecord(
    parsed.structuredContent ?? parsed.structured_content
  )
  const resultStructured = parseNestedRecord(
    result?.structuredContent ?? result?.structured_content
  )
  const delivery = extractDeliveryArtifact(output)
  const candidates = [
    structured?.message_id,
    structured?.messageId,
    parsed.message_id,
    parsed.messageId,
    resultStructured?.message_id,
    resultStructured?.messageId,
    result?.message_id,
    result?.messageId,
    delivery?.message_id,
    delivery?.messageId,
  ]
  return (
    candidates.find(
      (value): value is string =>
        typeof value === "string" && value.trim() !== ""
    ) ?? null
  )
}

function extractDeliveryArtifact(
  output: string | null | undefined
): Record<string, unknown> | null {
  const parsed = parseRecord(output)
  if (!parsed) return null
  const result = parseNestedRecord(parsed.result)
  const structured = parseNestedRecord(
    parsed.structuredContent ?? parsed.structured_content
  )
  const resultStructured = parseNestedRecord(
    result?.structuredContent ?? result?.structured_content
  )
  return (
    parseNestedRecord(parseNestedRecord(structured?.delivery)?.artifact) ??
    parseNestedRecord(
      parseNestedRecord(resultStructured?.delivery)?.artifact
    ) ??
    parseNestedRecord(parseNestedRecord(result?.delivery)?.artifact) ??
    parseNestedRecord(parseNestedRecord(parsed.delivery)?.artifact) ??
    null
  )
}

function indicatesRejectedArtifactCall(output: string): boolean {
  if (/Task artifact registration failed:/i.test(output)) return true
  return /Presented\s+0\s+task artifact\(s\)/i.test(output)
}

function acceptedPaths(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return value.flatMap((item) => {
    const record = parseNestedRecord(item)
    return typeof record?.path === "string" ? [record.path] : []
  })
}

function parseRecord(value: string | null | undefined) {
  if (!value) return null
  const direct = parseJsonRecord(value)
  if (direct) return direct
  const lines = value.split(/\r?\n/)
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const parsed = parseJsonRecord(lines[index].trim())
    if (parsed) return parsed
  }
  return null
}

function parseJsonRecord(value: string) {
  try {
    return parseNestedRecord(JSON.parse(value))
  } catch {
    return null
  }
}

function parseNestedRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value === "string") {
    try {
      return parseNestedRecord(JSON.parse(value))
    } catch {
      return null
    }
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) return null
  return value as Record<string, unknown>
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
