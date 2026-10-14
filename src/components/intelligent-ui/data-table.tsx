"use client"

import { useTranslations } from "next-intl"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { downloadUiFile } from "@/lib/intelligent-ui/export"
import { list, record, text, useViewState } from "./context"

type Column = { key: string; label: string }
type Order = { key: string; descending: boolean } | null
const PAGE_SIZE = 25
export function DataTable({ props }: { props: Record<string, unknown> }) {
  const t = useTranslations("IntelligentUI")
  const [query, setQuery] = useViewState<string>("query", "")
  const [sort, setSort] = useViewState<Order>("sort", null)
  const [page, setPage] = useViewState<number>("page", 0)
  const columns = list(props.columns)
    .slice(0, 20)
    .map((value): Column => {
      const c = record(value)
      return typeof value === "string"
        ? { key: value, label: value }
        : { key: text(c.key), label: text(c.label ?? c.key) }
    })
  const rows = tableRows(props.rows, query, sort),
    pages = Math.max(1, Math.ceil(rows.length / PAGE_SIZE)),
    current = Math.min(page, pages - 1)
  return (
    <div className="space-y-2">
      <TableToolbar
        query={query}
        columns={columns}
        rows={rows}
        change={(value) => {
          setQuery(value)
          setPage(0)
        }}
      />
      <div className="max-h-96 overflow-auto">
        <table className="w-full text-left text-xs">
          <TableHead columns={columns} sort={sort} change={setSort} />
          <TableBody
            columns={columns}
            rows={rows.slice(current * PAGE_SIZE, (current + 1) * PAGE_SIZE)}
          />
        </table>
        {!rows.length && (
          <p className="p-4 text-center text-muted-foreground">{t("empty")}</p>
        )}
      </div>
      <TablePagination current={current} pages={pages} change={setPage} />
    </div>
  )
}
function TableHead({
  columns,
  sort,
  change,
}: {
  columns: Column[]
  sort: Order
  change: (order: Order) => void
}) {
  return (
    <thead className="sticky top-0 bg-card">
      <tr>
        {columns.map((column) => (
          <th
            key={column.key}
            className="border-b px-2 py-2"
            aria-sort={
              sort?.key === column.key
                ? sort.descending
                  ? "descending"
                  : "ascending"
                : "none"
            }
          >
            <button
              onClick={() =>
                change({
                  key: column.key,
                  descending: sort?.key === column.key && !sort.descending,
                })
              }
            >
              {column.label} ↕
            </button>
          </th>
        ))}
      </tr>
    </thead>
  )
}
function TableBody({
  columns,
  rows,
}: {
  columns: Column[]
  rows: Record<string, unknown>[]
}) {
  return (
    <tbody>
      {rows.map((row, i) => (
        <tr key={i}>
          {columns.map((column, j) => (
            <td key={j} className="border-b px-2 py-2">
              {text(row[column.key])}
            </td>
          ))}
        </tr>
      ))}
    </tbody>
  )
}
function tableRows(raw: unknown, query: string, sort: Order) {
  const rows = list(raw)
    .slice(0, 1000)
    .map(record)
    .filter(
      (row) =>
        !query ||
        Object.values(row).some((value) =>
          text(value).toLowerCase().includes(query.toLowerCase())
        )
    )
  if (sort)
    rows.sort(
      (a, b) =>
        (typeof a[sort.key] === "number" && typeof b[sort.key] === "number"
          ? Number(a[sort.key]) - Number(b[sort.key])
          : text(a[sort.key]).localeCompare(text(b[sort.key]), undefined, {
              numeric: true,
            })) * (sort.descending ? -1 : 1)
    )
  return rows
}
function exportTable(columns: Column[], rows: Record<string, unknown>[]) {
  const cell = (value: unknown) =>
    `"${text(value)
      .replace(/^[=+@-]/, "'$&")
      .replace(/"/g, '""')}"`
  const content = [
    columns.map((column) => cell(column.label)).join(","),
    ...rows.map((row) =>
      columns.map((column) => cell(row[column.key])).join(",")
    ),
  ].join("\r\n")
  downloadUiFile({
    name: "table.csv",
    content: "\uFEFF" + content,
    type: "text/csv;charset=utf-8",
  })
}

function TableToolbar({
  query,
  columns,
  rows,
  change,
}: {
  query: string
  columns: Column[]
  rows: Record<string, unknown>[]
  change: (query: string) => void
}) {
  const t = useTranslations("IntelligentUI")
  return (
    <div className="flex gap-2">
      <Input
        aria-label={t("search")}
        placeholder={t("search")}
        value={query}
        maxLength={200}
        onChange={(event) => change(event.target.value)}
      />
      <Button
        size="sm"
        variant="outline"
        onClick={() => exportTable(columns, rows)}
      >
        {t("export")}
      </Button>
    </div>
  )
}
function TablePagination({
  current,
  pages,
  change,
}: {
  current: number
  pages: number
  change: (page: number) => void
}) {
  const t = useTranslations("IntelligentUI")
  return (
    <div className="flex items-center justify-end gap-2">
      <Button
        size="sm"
        variant="ghost"
        disabled={current === 0}
        onClick={() => change(current - 1)}
      >
        {t("previous")}
      </Button>
      <span className="text-xs tabular-nums">
        {current + 1} / {pages}
      </span>
      <Button
        size="sm"
        variant="ghost"
        disabled={current >= pages - 1}
        onClick={() => change(current + 1)}
      >
        {t("next")}
      </Button>
    </div>
  )
}
