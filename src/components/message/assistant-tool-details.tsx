import { MessageResponse } from "@/components/ai-elements/message"
import { MarkdownLink } from "@/components/ai-elements/markdown-link"
import { FileText } from "lucide-react"
import type { processToolDetails } from "./assistant-tool-sources"

export function AssistantToolDetails({
  details,
}: {
  details: NonNullable<ReturnType<typeof processToolDetails>>
}) {
  return (
    <div className="assistant-tool-panel">
      {details.sources.map((source) => (
        <div key={source.url} className="assistant-tool-source">
          <FileText aria-hidden="true" className="size-3.5 shrink-0" />
          <MarkdownLink
            href={source.url}
            className="assistant-tool-source-link"
          >
            {source.title}
          </MarkdownLink>
        </div>
      ))}
      {details.text && (
        <div className="assistant-tool-excerpt">
          <MessageResponse>{details.text}</MessageResponse>
        </div>
      )}
    </div>
  )
}
