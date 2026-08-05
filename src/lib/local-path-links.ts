export const LOCAL_PATH_URI_PREFIX = "iyw-claw://local-path/"

export type LocalPathPresentation = "text" | "inline-code"

export interface LocalPathMatch {
  start: number
  end: number
  path: string
}

const TRAILING_PUNCTUATION = new Set(
  Array.from(",.;:!?，。；：！？、)]}》】」』")
)

function isBoundary(value: string | undefined): boolean {
  return value == null || /[\s([{'"<,:;!?，；：！？、（【《「『]/.test(value)
}

export function isAbsoluteLocalPath(value: string): boolean {
  if (/^[a-zA-Z]:[\\/][^\r\n]+$/.test(value)) return true
  if (/^\\\\[^\\/\s]+[\\/][^\r\n]+$/.test(value)) return true
  if (!/^\/(?!\/)[^\r\n]+$/.test(value)) return false

  const body = value.slice(1)
  const slashCount = Array.from(body).filter((char) => char === "/").length
  return slashCount >= 2 || /\.[^/\s]+$/.test(body)
}

function trimCandidateEnd(text: string, start: number, end: number): number {
  let nextEnd = end
  while (
    nextEnd > start &&
    TRAILING_PUNCTUATION.has(text.charAt(nextEnd - 1))
  ) {
    nextEnd -= 1
  }
  return nextEnd
}

function quotedMatchAt(text: string, start: number): LocalPathMatch | null {
  const quote = text.charAt(start)
  if (quote !== '"' && quote !== "'") return null
  const end = text.indexOf(quote, start + 1)
  if (end < 0) return null
  const path = text.slice(start + 1, end)
  if (!isAbsoluteLocalPath(path)) return null
  return { start: start + 1, end, path }
}

function unquotedMatchAt(text: string, start: number): LocalPathMatch | null {
  if (!isBoundary(text[start - 1])) return null
  const tail = text.slice(start)
  const startsAbsolute =
    /^[a-zA-Z]:[\\/]/.test(tail) ||
    tail.startsWith("\\\\") ||
    (tail.startsWith("/") && !tail.startsWith("//"))
  if (!startsAbsolute) return null

  let end = start
  while (end < text.length && !/[\s<>"'`]/.test(text.charAt(end))) end += 1
  end = trimCandidateEnd(text, start, end)
  const path = text.slice(start, end)
  return isAbsoluteLocalPath(path) ? { start, end, path } : null
}

function wholeLineMatch(text: string): LocalPathMatch | null {
  const leading = text.length - text.trimStart().length
  let value = text.trim()
  let start = leading
  if (
    value.length >= 2 &&
    ((value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'")))
  ) {
    value = value.slice(1, -1)
    start += 1
  } else {
    const end = trimCandidateEnd(value, 0, value.length)
    value = value.slice(0, end)
  }
  if (!isAbsoluteLocalPath(value)) return null
  return { start, end: start + value.length, path: value }
}

export function findLocalPathMatches(
  text: string,
  options: { wholeLine?: boolean } = {}
): LocalPathMatch[] {
  if (options.wholeLine) {
    const wholeLine = wholeLineMatch(text)
    if (wholeLine) return [wholeLine]
  }

  const matches: LocalPathMatch[] = []
  let index = 0
  while (index < text.length) {
    const match = quotedMatchAt(text, index) ?? unquotedMatchAt(text, index)
    if (!match) {
      index += 1
      continue
    }
    matches.push(match)
    index = match.end
  }
  return matches
}

export function createLocalPathUri(
  path: string,
  presentation: LocalPathPresentation
): string {
  return `${LOCAL_PATH_URI_PREFIX}${encodeURIComponent(path)}?presentation=${presentation}`
}

export function parseLocalPathUri(
  uri: string
): { path: string; presentation: LocalPathPresentation } | null {
  if (!uri.toLowerCase().startsWith(LOCAL_PATH_URI_PREFIX)) return null
  const payload = uri.slice(LOCAL_PATH_URI_PREFIX.length)
  const queryIndex = payload.indexOf("?")
  const encodedPath = queryIndex >= 0 ? payload.slice(0, queryIndex) : payload
  const query = queryIndex >= 0 ? payload.slice(queryIndex + 1) : ""

  try {
    const presentation = new URLSearchParams(query).get("presentation")
    return {
      path: decodeURIComponent(encodedPath),
      presentation: presentation === "inline-code" ? "inline-code" : "text",
    }
  } catch {
    return null
  }
}
