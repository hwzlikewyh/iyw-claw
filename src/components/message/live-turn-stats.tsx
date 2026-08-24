import type {
  LiveContentBlock,
  LiveMessage,
} from "@/contexts/acp-connections-context"
import { inferLiveToolName } from "@/lib/tool-call-normalization"
import {
  countUnifiedDiffLineChanges,
  estimateChangedLineStats,
} from "@/lib/line-change-stats"

interface LineChangeStats {
  additions: number
  deletions: number
}

interface LiveEditStats extends LineChangeStats {
  files: number
}

function asObject(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null
  return value as Record<string, unknown>
}

function parseInputObject(
  input: string | null
): Record<string, unknown> | null {
  if (!input) return null
  try {
    return asObject(JSON.parse(input))
  } catch {
    return null
  }
}

function unescapeInlineEscapes(text: string): string {
  return text
    .replace(/\\r\\n/g, "\n")
    .replace(/\\n/g, "\n")
    .replace(/\\t/g, "\t")
}

function looksLikeDiffPayload(input: string): boolean {
  const normalized = unescapeInlineEscapes(input)
  return (
    normalized.includes("*** Begin Patch") ||
    normalized.includes("*** Update File:") ||
    /^diff --git /m.test(normalized) ||
    (/^--- .+/m.test(normalized) && /^\+\+\+ .+/m.test(normalized)) ||
    /^@@ /m.test(normalized)
  )
}

function extractPatchText(
  rawInput: string | null,
  parsed: Record<string, unknown> | null
): string | null {
  if (!rawInput) return null
  if (looksLikeDiffPayload(rawInput)) return unescapeInlineEscapes(rawInput)
  if (!parsed) return null

  const candidates = [
    parsed.patch,
    parsed.diff,
    parsed.unified_diff,
    parsed.unifiedDiff,
    parsed.command,
    parsed.input,
    parsed.arguments,
    parsed.payload,
  ]

  for (const candidate of candidates) {
    if (typeof candidate !== "string") continue
    if (looksLikeDiffPayload(candidate)) return unescapeInlineEscapes(candidate)
  }

  return null
}

function addPathIfValid(paths: Set<string>, value: unknown): void {
  if (typeof value !== "string") return
  const path = value.trim()
  if (!path) return
  paths.add(path)
}

function collectParsedPaths(
  parsed: Record<string, unknown> | null
): Set<string> {
  const paths = new Set<string>()
  if (!parsed) return paths

  addPathIfValid(
    paths,
    parsed.file_path ?? parsed.filePath ?? parsed.path ?? parsed.notebook_path
  )

  const changes = asObject(parsed.changes)
  if (changes) {
    for (const path of Object.keys(changes)) {
      addPathIfValid(paths, path)
    }
  }

  return paths
}

function parseApplyPatchStats(patch: string): {
  files: Set<string>
  additions: number
  deletions: number
} {
  const files = new Set<string>()
  let additions = 0
  let deletions = 0

  for (const line of patch.split("\n")) {
    if (line.startsWith("*** Add File: ")) {
      addPathIfValid(files, line.slice(14))
      continue
    }
    if (line.startsWith("*** Update File: ")) {
      addPathIfValid(files, line.slice(17))
      continue
    }
    if (line.startsWith("*** Delete File: ")) {
      addPathIfValid(files, line.slice(17))
      continue
    }
    if (line.startsWith("+++ ")) {
      const normalized = line.slice(4).replace(/^b\//, "").trim()
      if (normalized && normalized !== "/dev/null") {
        files.add(normalized)
      }
      continue
    }
    if (line.startsWith("+") && !line.startsWith("+++")) additions += 1
    if (line.startsWith("-") && !line.startsWith("---")) deletions += 1
  }

  return { files, additions, deletions }
}

function extractEditStats(parsed: Record<string, unknown>): LineChangeStats {
  const changes = asObject(parsed.changes)
  if (changes) {
    let additions = 0
    let deletions = 0

    for (const change of Object.values(changes)) {
      const record = asObject(change)
      if (!record) continue

      const unifiedDiff =
        (typeof record.unifiedDiff === "string" && record.unifiedDiff) ||
        (typeof record.unified_diff === "string" && record.unified_diff) ||
        null

      if (unifiedDiff) {
        const stats = countUnifiedDiffLineChanges(unifiedDiff)
        additions += stats.additions
        deletions += stats.deletions
        continue
      }

      const oldString =
        (typeof record.oldText === "string" && record.oldText) ||
        (typeof record.old_string === "string" && record.old_string) ||
        ""
      const newString =
        (typeof record.newText === "string" && record.newText) ||
        (typeof record.new_string === "string" && record.new_string) ||
        ""

      const estimated = estimateChangedLineStats(oldString, newString)
      additions += estimated.additions
      deletions += estimated.deletions
    }

    return { additions, deletions }
  }

  const oldString =
    (typeof parsed.old_string === "string" && parsed.old_string) ||
    (typeof parsed.oldText === "string" && parsed.oldText) ||
    ""
  const newString =
    (typeof parsed.new_string === "string" && parsed.new_string) ||
    (typeof parsed.newText === "string" && parsed.newText) ||
    ""

  return estimateChangedLineStats(oldString, newString)
}

function extractWriteStats(parsed: Record<string, unknown>): LineChangeStats {
  const content =
    (typeof parsed.content === "string" && parsed.content) ||
    (typeof parsed.new_source === "string" && parsed.new_source) ||
    ""

  const additions = content.length === 0 ? 0 : content.split("\n").length
  return { additions, deletions: 0 }
}

interface BlockEditContribution {
  files: string[]
  additions: number
  deletions: number
}

const blockEditContributionCache = new WeakMap<
  LiveContentBlock,
  BlockEditContribution | null
>()

function computeBlockEditContribution(
  block: LiveContentBlock
): BlockEditContribution | null {
  if (block.type !== "tool_call") return null
  const toolName = inferLiveToolName({
    title: block.info.title,
    kind: block.info.kind,
    rawInput: block.info.raw_input,
    meta: block.info.meta,
  })
  if (toolName !== "edit" && toolName !== "write" && toolName !== "apply_patch")
    return null

  const files = new Set<string>()
  let additions = 0
  let deletions = 0

  const parsed = parseInputObject(block.info.raw_input)
  for (const path of collectParsedPaths(parsed)) files.add(path)

  if (toolName === "apply_patch") {
    const patch = extractPatchText(block.info.raw_input, parsed)
    if (patch) {
      const stats = parseApplyPatchStats(patch)
      for (const path of stats.files) files.add(path)
      additions += stats.additions
      deletions += stats.deletions
    }
  } else if (parsed) {
    const stats =
      toolName === "edit" ? extractEditStats(parsed) : extractWriteStats(parsed)
    additions += stats.additions
    deletions += stats.deletions
  }

  return { files: [...files], additions, deletions }
}

function blockEditContribution(
  block: LiveContentBlock
): BlockEditContribution | null {
  const cached = blockEditContributionCache.get(block)
  if (cached !== undefined) return cached
  const contribution = computeBlockEditContribution(block)
  blockEditContributionCache.set(block, contribution)
  return contribution
}

export function extractLiveEditStats(message: LiveMessage): LiveEditStats {
  const files = new Set<string>()
  let additions = 0
  let deletions = 0

  for (const block of message.content) {
    const contribution = blockEditContribution(block)
    if (!contribution) continue
    for (const path of contribution.files) files.add(path)
    additions += contribution.additions
    deletions += contribution.deletions
  }

  return { files: files.size, additions, deletions }
}

export { LiveTurnStats } from "@/components/message/live-turn-stats-view"
