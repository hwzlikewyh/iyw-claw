import {
  buildDirectoryUri,
  buildFileUri,
  isAbsoluteFilesystemPath,
  LOCAL_DIRECTORY_PROMPT_PREFIX,
  LOCAL_FILE_PROMPT_PREFIX,
} from "./reference-link"

export type LocalFilePromptSegment =
  | { type: "text"; value: string }
  | { type: "file"; path: string; fileKind: "file" | "dir" }

const LINE_RANGE_FRAGMENT = /#L(\d+)(?:-(\d+))?$/

function readInlineCode(
  text: string,
  start: number
): { value: string; end: number } | null {
  if (text[start] !== "`") return null
  let fenceEnd = start
  while (text[fenceEnd] === "`") fenceEnd += 1
  const fence = text.slice(start, fenceEnd)
  const close = text.indexOf(fence, fenceEnd)
  if (close < 0) return null

  let value = text.slice(fenceEnd, close)
  if (
    value.length >= 2 &&
    value.startsWith(" ") &&
    value.endsWith(" ") &&
    value.trim().length > 0
  ) {
    value = value.slice(1, -1)
  }
  return { value, end: close + fence.length }
}

function nextPromptMarker(
  text: string,
  start: number
): { index: number; prefix: string; fileKind: "file" | "dir" } | null {
  const candidates = [
    {
      index: text.indexOf(LOCAL_FILE_PROMPT_PREFIX, start),
      prefix: LOCAL_FILE_PROMPT_PREFIX,
      fileKind: "file" as const,
    },
    {
      index: text.indexOf(LOCAL_DIRECTORY_PROMPT_PREFIX, start),
      prefix: LOCAL_DIRECTORY_PROMPT_PREFIX,
      fileKind: "dir" as const,
    },
  ].filter((candidate) => candidate.index >= 0)
  return candidates.sort((a, b) => a.index - b.index)[0] ?? null
}

export function splitLocalFilePrompts(text: string): LocalFilePromptSegment[] {
  const segments: LocalFilePromptSegment[] = []
  let textStart = 0
  let searchStart = 0
  while (searchStart < text.length) {
    const marker = nextPromptMarker(text, searchStart)
    if (!marker) break
    const code = readInlineCode(text, marker.index + marker.prefix.length)
    if (!code || !isAbsoluteFilesystemPath(code.value)) {
      searchStart = marker.index + marker.prefix.length
      continue
    }
    if (marker.index > textStart) {
      segments.push({
        type: "text",
        value: text.slice(textStart, marker.index),
      })
    }
    segments.push({ type: "file", path: code.value, fileKind: marker.fileKind })
    textStart = code.end
    searchStart = code.end
  }
  if (textStart < text.length) {
    segments.push({ type: "text", value: text.slice(textStart) })
  }
  return segments.length > 0 ? segments : [{ type: "text", value: text }]
}

export function localFilePromptLabel(path: string): string {
  const range = path.match(LINE_RANGE_FRAGMENT)
  const withoutRange = range ? path.slice(0, range.index) : path
  const name = withoutRange.split(/[\\/]/).filter(Boolean).pop() || withoutRange
  if (!range) return name
  return range[2] ? `${name}:${range[1]}-${range[2]}` : `${name}:${range[1]}`
}

export function localFilePromptUri(
  path: string,
  fileKind: "file" | "dir" = "file"
): string {
  const range = path.match(LINE_RANGE_FRAGMENT)
  const withoutRange = range ? path.slice(0, range.index) : path
  const uri =
    fileKind === "dir"
      ? buildDirectoryUri(withoutRange)
      : buildFileUri(withoutRange)
  return range ? `${uri}${range[0]}` : uri
}

export function localFilePromptsToNames(text: string): string {
  return splitLocalFilePrompts(text)
    .map((segment) =>
      segment.type === "file"
        ? localFilePromptLabel(segment.path)
        : segment.value
    )
    .join("")
}
