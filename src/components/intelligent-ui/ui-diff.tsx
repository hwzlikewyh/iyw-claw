import { UnifiedDiffPreview } from "@/components/diff/unified-diff-preview"
import { generateUnifiedDiff } from "@/lib/unified-diff-generator"
import { text } from "./context"

export function Diff({ props }: { props: Record<string, unknown> }) {
  const before = text(props.before ?? ""),
    after = text(props.after ?? "")
  const diff = generateUnifiedDiff(before, after, "content")
  return diff ? (
    <UnifiedDiffPreview diffText={diff} />
  ) : (
    <pre className="max-h-96 overflow-auto whitespace-pre-wrap rounded border p-3 text-xs">
      {before}
    </pre>
  )
}
