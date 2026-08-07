import { fromMarkdown } from "mdast-util-from-markdown"
import {
  localFilePromptLabel,
  localFilePromptsToNames,
} from "./local-file-prompt"
import {
  fileUriToPath,
  tokenizeReferenceLinks,
  unescapeReferenceLabel,
} from "./reference-link"

/**
 * Reverse the composer's send-time text escaping so a copied USER message reads
 * literally instead of carrying serialization artifacts — while keeping code and
 * non-local reference links byte-for-byte intact.
 *
 * When plain text is typed into the composer, `@tiptap/markdown`'s
 * `encodeTextForMarkdown` runs `escapeMarkdownSyntax(encodeHtmlEntities(text))`
 * over every NON-CODE text node before the message is sent and stored: HTML-
 * significant chars become entities (`&` → `&amp;`, `<` → `&lt;`, `>` → `&gt;`)
 * and Markdown-significant chars get a leading backslash (`\` → `\\`, `*` → `\*`,
 * …). Code marks / fenced code blocks are emitted verbatim. The transcript renders
 * the stored Markdown through a Markdown renderer (which undoes the escaping in
 * prose and shows code / links as authored), but the copy button copies the raw
 * stored string — so a typed Windows path `C:\tools\x.ts` would otherwise paste as
 * `C:\\tools\\x.ts`.
 *
 * To recover the literal text in EVERY context — including code nested in
 * blockquotes or lists, which a line scanner can't disambiguate from prose without
 * full container parsing — we parse the Markdown with the same CommonMark parser
 * family the transcript renders with ({@link fromMarkdown}), keep every `code` /
 * `inlineCode` / `link` node's source span verbatim, and reverse the escaping only
 * in the prose between them. Local file links are replaced by their labels so a
 * copied user message never exposes the filesystem path; other links stay verbatim
 * so an escaped reference label like `[a\]b.ts](…)` isn't corrupted and the
 * destination a paste needs is kept; a
 * user-typed literal `\[foo\]` has escaped brackets, is not a link, and so still
 * unescapes to `[foo]`.
 *
 * Apply ONLY to user text: assistant messages are the agent's own Markdown, where
 * a `\*` may be an intentional literal asterisk that must survive a copy.
 */
export function unescapeComposerText(text: string): string {
  const displayText = localFilePromptsToNames(text)
  // Running a full CommonMark parser in the click handler exposes the copy path to
  // its worst-case latency: parse cost grows with input size, block count (many
  // list items / paragraphs), and inline-markup density (`` ` `` / `*` / `[` / `<`
  // storms), and pathological inputs take seconds. A hand-authored message is far
  // below every bound, so when one is exceeded we skip parsing and return the raw
  // text unchanged — safe (code is never corrupted) and bounded, at the cost of not
  // unescaping that one rare oversized / marker-storm message.
  if (tooExpensiveToParse(displayText)) {
    return localFileLinksToNames(displayText)
  }

  let ranges: VerbatimRange[]
  try {
    ranges = collectVerbatimRanges(fromMarkdown(displayText))
  } catch {
    return localFileLinksToNames(displayText)
  }
  if (ranges.length === 0) return unescapeComposerProse(displayText)
  ranges.sort((a, b) => a.start - b.start)

  let out = ""
  let cursor = 0
  for (const { start, end, replacement } of ranges) {
    if (end <= cursor) continue // fully inside a span already emitted
    const verbatimStart = Math.max(start, cursor)
    if (verbatimStart > cursor) {
      out += unescapeComposerProse(displayText.slice(cursor, verbatimStart))
    }
    out += replacement ?? displayText.slice(verbatimStart, end)
    cursor = end
  }
  if (cursor < displayText.length) {
    out += unescapeComposerProse(displayText.slice(cursor))
  }
  return out
}

// Parse-cost ceilings, each far above any hand-authored chat message but low
// enough to keep even a worst-case shape's parse well under ~1s. A message past
// any one of these falls back to the raw text. (Measured warm: ~2000 list items
// ≈ 250ms, 128 KB plain ≈ 110ms, a `<`/backtick/emphasis storm at 2000 delimiters
// ≈ 100ms.)
const MAX_PARSE_BYTES = 128 * 1024
const MAX_PARSE_LINES = 2000
const MAX_INLINE_DELIMITERS = 2000

/**
 * Whether `text` is too large / block-heavy / marker-dense to parse cheaply. The
 * length check is O(1) and bounds the subsequent scan, which early-exits on the
 * line and delimiter budgets — so this stays O(min(length, budget)) on any input.
 * `<` / `>` count as delimiters because they drive the parser's autolink/HTML
 * scanning (and the serializer encodes real prose `<` as `&lt;`, so a raw run is
 * already a code/paste artifact).
 */
function tooExpensiveToParse(text: string): boolean {
  if (text.length > MAX_PARSE_BYTES) return true
  let lines = 0
  let delimiters = 0
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i]
    if (c === "\n") {
      if (++lines > MAX_PARSE_LINES) return true
    } else {
      switch (c) {
        case "`":
        case "*":
        case "_":
        case "~":
        case "[":
        case "]":
        case "<":
        case ">":
          if (++delimiters > MAX_INLINE_DELIMITERS) return true
      }
    }
  }
  return false
}

/** Minimal structural view of the mdast nodes we walk — avoids a hard dependency
 *  on `@types/mdast` while staying type-safe for the offsets/children we read. */
interface MdastNode {
  type: string
  url?: string
  value?: string
  position?: {
    start: { offset?: number | null }
    end: { offset?: number | null }
  }
  children?: MdastNode[]
}

/** mdast node types whose source span is copied through verbatim (never escaped). */
const VERBATIM_NODE_TYPES = new Set(["code", "inlineCode", "link"])

interface VerbatimRange {
  start: number
  end: number
  replacement?: string
}

function safeDecodedBasename(value: string): string | null {
  let decoded = value
  try {
    decoded = decodeURIComponent(value)
  } catch {
    decoded = value.replace(/%2f/gi, "/").replace(/%5c/gi, "\\")
  }
  const withoutSuffix = decoded.split(/[?#]/, 1)[0]
  const candidate = withoutSuffix.split(/[\\/]/).filter(Boolean).pop() ?? ""
  const safe = candidate.replace(/[\r\n]+/g, " ").trim()
  return safe && !/^(?:file:|[a-z]:)/i.test(safe) ? safe : null
}

function localFileDisplayName(
  label: string,
  uri: string,
  path: string | null
): string {
  return (
    safeDecodedBasename(label) ??
    (path ? localFilePromptLabel(path) : null) ??
    safeDecodedBasename(uri.replace(/^file:\/\/+?/i, "")) ??
    "file"
  )
}

function localFileLinkName(node: MdastNode): string | undefined {
  if (node.type !== "link" || !node.url) return undefined
  if (!/^file:\/\//i.test(node.url)) return undefined
  const path = fileUriToPath(node.url)
  const label = node.children
    ?.map((child) => child.value ?? "")
    .join("")
    .trim()
  return localFileDisplayName(label ?? "", node.url, path)
}

/**
 * Source offset ranges of every `code` / `inlineCode` / `link` node. A verbatim
 * node's children are subsumed by its own span (e.g. inline code inside a link),
 * so the walk stops descending into it — keeping the returned ranges disjoint.
 */
function collectVerbatimRanges(root: unknown): VerbatimRange[] {
  const ranges: VerbatimRange[] = []
  const visit = (node: MdastNode): void => {
    if (VERBATIM_NODE_TYPES.has(node.type)) {
      const start = node.position?.start.offset
      const end = node.position?.end.offset
      if (typeof start === "number" && typeof end === "number") {
        ranges.push({
          start,
          end,
          replacement: localFileLinkName(node),
        })
        return
      }
    }
    if (node.children) for (const child of node.children) visit(child)
  }
  visit(root as MdastNode)
  return ranges
}

function localFileLinksToNames(text: string): string {
  return tokenizeReferenceLinks(text)
    .map((token) => {
      if (token.type === "text") return token.value
      const destination = token.destination
        .replace(/^<|>$/g, "")
        .replace(/\\([\\<>])/g, "$1")
      if (!/^file:\/\//i.test(destination)) return token.raw
      const path = fileUriToPath(destination)
      return localFileDisplayName(
        unescapeReferenceLabel(token.label),
        destination,
        path
      )
    })
    .join("")
}

/**
 * Reverse {@link unescapeComposerText}'s escaping for a single prose (non-code,
 * non-link) run: decode the HTML entities in encode-reverse order (so `&amp;`
 * decodes last and a literal `&lt;`, stored as `&amp;lt;`, round-trips to `&lt;`
 * rather than `<`), then drop the escaping backslash from the exact
 * `escapeMarkdownSyntax` character class. The two passes touch disjoint
 * characters, so the order between them is immaterial.
 */
function unescapeComposerProse(text: string): string {
  return text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/\\([\\`*_[\]~])/g, "$1")
}
