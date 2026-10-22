import { assistantResponseFirstLine } from "./assistant-text-segments"

export function AssistantFinalResponsePreview({
  parts,
}: {
  parts: { text: string }[]
}) {
  const summary = assistantResponseFirstLine(parts.map((part) => part.text))
  if (!summary) return null
  return <p className="assistant-final-response-preview">{summary}</p>
}
