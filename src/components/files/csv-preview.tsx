"use client"

import { useMemo } from "react"
import type { PreviewState } from "@/components/message/workspace-file-preview"

const MAX_ROWS = 5000
const MAX_COLUMNS = 100

export function CsvPreview({
  state,
}: {
  state: Extract<PreviewState, { status: "csv" }>
}) {
  const parsed = useMemo(() => parseDelimited(state.content, state.delimiter), [state.content, state.delimiter])
  const columns = Math.min(parsed.columns, MAX_COLUMNS)
  const rows = parsed.rows.slice(0, MAX_ROWS)
  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div className="min-h-0 flex-1 overflow-auto">
        <table data-artifact-preview-text className="w-full min-w-max border-collapse text-xs">
          <thead className="sticky top-0 z-10 bg-muted text-left">
            <tr>{Array.from({ length: columns }, (_, index) => <th key={index} className="border-b border-r px-3 py-2 font-medium">{rows[0]?.[index] || `列 ${index + 1}`}</th>)}</tr>
          </thead>
          <tbody>
            {rows.slice(1).map((row, rowIndex) => (
              <tr key={rowIndex} className="odd:bg-muted/20">
                {Array.from({ length: columns }, (_, index) => <td key={index} className="max-w-80 border-b border-r px-3 py-2 align-top whitespace-pre-wrap break-words">{row[index] ?? ""}</td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {(parsed.truncated || state.truncated) && <div className="border-t bg-muted/20 px-4 py-2 text-xs text-muted-foreground">仅显示前 {MAX_ROWS} 行或文件限制范围内内容</div>}
    </div>
  )
}

function parseDelimited(content: string, delimiter: "," | "\t") {
  const rows: string[][] = []
  let row: string[] = []
  let cell = ""
  let quoted = false
  for (let index = 0; index < content.length; index += 1) {
    const char = content[index]
    if (char === '"') {
      if (quoted && content[index + 1] === '"') { cell += '"'; index += 1 } else quoted = !quoted
    } else if (char === delimiter && !quoted) { row.push(cell); cell = "" }
    else if ((char === "\n" || char === "\r") && !quoted) {
      if (char === "\r" && content[index + 1] === "\n") index += 1
      row.push(cell); rows.push(row); row = []; cell = ""
      if (rows.length >= MAX_ROWS + 1) break
    } else cell += char
  }
  if (cell || row.length) { row.push(cell); rows.push(row) }
  return { rows, columns: rows.reduce((max, current) => Math.max(max, current.length), 0), truncated: content.length > 2 * 1024 * 1024 }
}
