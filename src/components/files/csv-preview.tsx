"use client"

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ClipboardEvent,
  type RefObject,
} from "react"
import {
  Virtualizer,
  type CustomContainerComponentProps,
  type CustomItemComponentProps,
} from "virtua"
import type { PreviewState } from "@/components/message/workspace-file-preview"
import { selectedPreviewText } from "@/lib/preview-selection"

const MAX_ROWS = 5000
const MAX_COLUMNS = 100
const COLUMN_WIDTH = 160
const MAX_SOURCE_CHARS = 2 * 1024 * 1024
type CsvState = Extract<PreviewState, { status: "csv" }>

function CsvBody({ ref, style, children }: CustomContainerComponentProps) {
  return (
    <tbody ref={ref} style={{ ...style, display: "block" }}>
      {children}
    </tbody>
  )
}

function CsvRow({ ref, style, index, children }: CustomItemComponentProps) {
  return (
    <tr
      ref={ref}
      aria-rowindex={index + 2}
      className={index % 2 === 0 ? "bg-muted/20" : undefined}
      style={{
        ...style,
        display: "table",
        tableLayout: "fixed",
        width: "100%",
      }}
    >
      {children}
    </tr>
  )
}

function copySelection(event: ClipboardEvent<HTMLDivElement>) {
  const text = selectedPreviewText(event.currentTarget)
  if (!text) return
  event.clipboardData.setData("text/plain", text)
  event.preventDefault()
}

export function CsvPreview({ state }: { state: CsvState }) {
  const parsed = useMemo(
    () => parseDelimited(state.content, state.delimiter),
    [state.content, state.delimiter]
  )
  const columns = Math.min(parsed.columns, MAX_COLUMNS)
  const rows = useMemo(() => parsed.rows.slice(1, MAX_ROWS), [parsed])
  const copySource = useMemo(
    () =>
      parsed.rows
        .slice(0, MAX_ROWS)
        .map((row) => row.slice(0, columns).join("\t"))
        .join("\n"),
    [parsed, columns]
  )
  return (
    <div
      className="flex h-full min-h-0 flex-col overflow-hidden"
      onCopy={copySelection}
    >
      <textarea
        hidden
        readOnly
        data-artifact-preview-source
        value={copySource}
      />
      <CsvTable
        rows={rows}
        header={parsed.rows[0] ?? []}
        columns={columns}
        path={state.path}
      />
      {(parsed.truncated || state.truncated) && (
        <div className="border-t bg-muted/20 px-4 py-2 text-xs text-muted-foreground">
          仅显示前 {MAX_ROWS} 行或文件限制范围内内容
        </div>
      )}
    </div>
  )
}

function useHeaderHeight() {
  const headerRef = useRef<HTMLTableSectionElement>(null)
  const [headerHeight, setHeaderHeight] = useState(0)
  useEffect(() => {
    const header = headerRef.current
    if (!header) return
    const observer = new ResizeObserver(() =>
      setHeaderHeight(header.offsetHeight)
    )
    observer.observe(header)
    return () => observer.disconnect()
  }, [])
  return { headerRef, headerHeight }
}

function CsvHeader({
  headerRef,
  header,
  columns,
}: {
  headerRef: RefObject<HTMLTableSectionElement | null>
  header: string[]
  columns: number
}) {
  return (
    <thead
      ref={headerRef}
      className="sticky top-0 z-10 block bg-muted text-left"
    >
      <tr className="table w-full table-fixed">
        {Array.from({ length: columns }, (_, index) => (
          <th
            key={index}
            scope="col"
            className="border-b border-r px-3 py-2 font-medium break-words"
          >
            {header[index] || "列 " + (index + 1)}
          </th>
        ))}
      </tr>
    </thead>
  )
}

function CsvCells({ row, columns }: { row: string[]; columns: number }) {
  return Array.from({ length: columns }, (_, index) => (
    <td
      key={index}
      className="border-b border-r px-3 py-2 align-top whitespace-pre-wrap break-words"
    >
      {row[index] ?? ""}
    </td>
  ))
}

function CsvTable({
  rows,
  header,
  columns,
  path,
}: {
  rows: string[][]
  header: string[]
  columns: number
  path: string
}) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const { headerRef, headerHeight } = useHeaderHeight()
  return (
    <div
      ref={scrollRef}
      tabIndex={0}
      className="min-h-0 flex-1 overflow-auto [overflow-anchor:none]"
    >
      <table
        data-artifact-preview-text
        aria-rowcount={rows.length + 1}
        className="block w-full border-collapse text-xs"
        style={{ minWidth: columns * COLUMN_WIDTH }}
      >
        <CsvHeader headerRef={headerRef} header={header} columns={columns} />
        <Virtualizer
          key={path}
          as={CsvBody}
          item={CsvRow}
          data={rows}
          scrollRef={scrollRef}
          startMargin={headerHeight}
        >
          {(row) => <CsvCells row={row} columns={columns} />}
        </Virtualizer>
      </table>
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
      if (quoted && content[index + 1] === '"') {
        cell += '"'
        index += 1
      } else quoted = !quoted
    } else if (char === delimiter && !quoted) {
      row.push(cell)
      cell = ""
    } else if ((char === "\n" || char === "\r") && !quoted) {
      if (char === "\r" && content[index + 1] === "\n") index += 1
      row.push(cell)
      rows.push(row)
      row = []
      cell = ""
      if (rows.length >= MAX_ROWS + 1) break
    } else cell += char
  }
  if (cell || row.length) {
    row.push(cell)
    rows.push(row)
  }
  return {
    rows,
    columns: rows.reduce((max, current) => Math.max(max, current.length), 0),
    truncated: rows.length > MAX_ROWS || content.length > MAX_SOURCE_CHARS,
  }
}
