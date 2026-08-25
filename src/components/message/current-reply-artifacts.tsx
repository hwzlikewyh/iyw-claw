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
  conversationId: number
  parts: AdaptedContentPart[]
}

interface ArtifactToolCall {
  input: string | null
  output?: string | null
}

interface ArtifactRegistration {
  hasCall: boolean
  rejected: boolean
  references: string[]
}

const PRESENT_TASK_FILES_SUFFIX = /present[_-]task[_-]files$/i

export const CurrentReplyArtifacts = memo(function CurrentReplyArtifacts({
  conversationId,
  parts,
}: CurrentReplyArtifactsProps) {
  const registration = useMemo(
    () => extractArtifactRegistration(parts),
    [parts]
  )

  return (
    <ResolvedReplyArtifacts
      conversationId={conversationId}
      registration={registration}
    />
  )
})

function ResolvedReplyArtifacts({
  conversationId,
  registration,
}: {
  conversationId: number
  registration: ArtifactRegistration
}) {
  const { activeFolder } = useActiveFolder()
  const query = useTaskArtifacts({
    conversationId,
    folderId: null,
    scope: "current",
    latestTurnOnly: !registration.hasCall,
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

function extractArtifactRegistration(
  parts: AdaptedContentPart[]
): ArtifactRegistration {
  const calls: ArtifactToolCall[] = []
  collectArtifactToolCalls(parts, calls)
  const references: string[] = []
  let rejected = false
  for (const call of calls) {
    if (call.output && indicatesRejectedArtifactCall(call.output)) {
      rejected = true
      continue
    }
    const accepted = extractAcceptedPaths(call.output)
    references.push(...(accepted ?? extractInputPaths(call.input)))
  }
  return {
    hasCall: calls.length > 0,
    rejected,
    references: dedupeStrings(references),
  }
}

function collectArtifactToolCalls(
  parts: AdaptedContentPart[],
  calls: ArtifactToolCall[]
): void {
  for (const part of parts) {
    if (part.type === "tool-call" && isArtifactToolCall(part)) {
      calls.push({ input: part.input, output: part.output })
    } else if (part.type === "tool-group") {
      collectArtifactToolCalls(part.items, calls)
    } else if (part.type === "goal-run") {
      collectArtifactToolCalls([part.start, ...part.items], calls)
      if (part.end) collectArtifactToolCalls([part.end], calls)
    }
  }
}

function isArtifactToolCall(call: ArtifactToolCall & { toolName: string }) {
  if (PRESENT_TASK_FILES_SUFFIX.test(call.toolName.trim())) return true
  const input = parseRecord(call.input)
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
  const accepted =
    structured?.accepted ??
    parsed.accepted ??
    resultStructured?.accepted ??
    result?.accepted
  return Array.isArray(accepted) ? acceptedPaths(accepted) : null
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
