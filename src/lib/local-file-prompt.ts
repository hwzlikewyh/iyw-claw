import { buildFileUri } from "./reference-link"

export const LOCAL_FILE_PROMPT_PREFIX =
  "Local file path (use filesystem tools, not MCP resources): "

export type LocalFilePromptSegment =
  | { type: "text"; value: string }
  | { type: "file"; path: string }

const LINE_RANGE_FRAGMENT = /#L(\d+)(?:-(\d+))?$/

function isAbsoluteLocalPath(path: string): boolean {
  return (
    /^[a-zA-Z]:[\\/]/.test(path) ||
    path.startsWith("\\\\") ||
    path.startsWith("/")
  )
}

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

export function splitLocalFilePrompts(text: string): LocalFilePromptSegment[] {
  const segments: LocalFilePromptSegment[] = []
  let cursor = 0
  while (cursor < text.length) {
    const marker = text.indexOf(LOCAL_FILE_PROMPT_PREFIX, cursor)
    if (marker < 0) break
    const code = readInlineCode(text, marker + LOCAL_FILE_PROMPT_PREFIX.length)
    if (!code || !isAbsoluteLocalPath(code.value)) {
      cursor = marker + LOCAL_FILE_PROMPT_PREFIX.length
      continue
    }
    if (marker > cursor) {
      segments.push({ type: "text", value: text.slice(cursor, marker) })
    }
    segments.push({ type: "file", path: code.value })
    cursor = code.end
  }
  if (cursor < text.length) {
    segments.push({ type: "text", value: text.slice(cursor) })
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

export function localFilePromptUri(path: string): string {
  const range = path.match(LINE_RANGE_FRAGMENT)
  const withoutRange = range ? path.slice(0, range.index) : path
  const uri = buildFileUri(withoutRange)
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
