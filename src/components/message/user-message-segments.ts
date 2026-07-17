import { parseIywClawReferenceUri } from "@/components/chat/composer/reference-uri"
import type { ReferenceAttrs } from "@/components/chat/composer/types"
import { INVOCATION_TOKEN_RE } from "@/lib/invocation-token"
import {
  tokenizeReferenceLinks,
  unescapeReferenceLabel,
} from "@/lib/reference-link"

export type UserMessageSegment =
  | { kind: "text"; text: string }
  | { kind: "reference"; attrs: ReferenceAttrs }

const REFERENCE_SCHEME = /^(?:file:|iyw-claw:)/i

function unwrapDestination(destination: string): string {
  const trimmed = destination.trim()
  return trimmed.startsWith("<") && trimmed.endsWith(">")
    ? trimmed.slice(1, -1).trim()
    : trimmed
}

function pushProseSegments(value: string, output: UserMessageSegment[]): void {
  INVOCATION_TOKEN_RE.lastIndex = 0
  let lastIndex = 0
  let match: RegExpExecArray | null
  while ((match = INVOCATION_TOKEN_RE.exec(value)) !== null) {
    const token = match[2]
    const tokenStart = match.index + match[1].length
    if (tokenStart > lastIndex) {
      output.push({ kind: "text", text: value.slice(lastIndex, tokenStart) })
    }
    const slug = token.slice(1)
    const attrs = parseIywClawReferenceUri(
      `iyw-claw://skill/${encodeURIComponent(slug)}`,
      token
    )
    output.push(
      attrs ? { kind: "reference", attrs } : { kind: "text", text: token }
    )
    lastIndex = INVOCATION_TOKEN_RE.lastIndex
  }
  if (lastIndex < value.length) {
    output.push({ kind: "text", text: value.slice(lastIndex) })
  }
}

export function parseUserMessageSegments(text: string): UserMessageSegment[] {
  const output: UserMessageSegment[] = []
  for (const token of tokenizeReferenceLinks(text)) {
    if (token.type === "link") {
      const destination = unwrapDestination(token.destination)
      if (REFERENCE_SCHEME.test(destination)) {
        const attrs = parseIywClawReferenceUri(
          destination,
          unescapeReferenceLabel(token.label)
        )
        if (attrs) {
          output.push({ kind: "reference", attrs })
          continue
        }
      }
      output.push({ kind: "text", text: token.raw })
      continue
    }
    pushProseSegments(token.value, output)
  }
  return output
}
