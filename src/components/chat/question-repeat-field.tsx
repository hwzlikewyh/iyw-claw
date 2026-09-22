import { useTranslations } from "next-intl"
import { Plus, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  MAX_ANSWER_CHARS,
  MAX_REPEAT_ROWS,
  repeatedRows,
} from "@/lib/question-answer"
import type { QuestionColumn } from "@/lib/question-ui"
import type { QuestionFieldProps } from "./question-field-props"

export function QuestionRepeatField({
  question,
  value,
  locked,
  onText,
}: QuestionFieldProps) {
  const rows = repeatedRows(value.otherText)
  const columns = question.ui?.columns ?? []
  const maxRows = Math.min(
    question.ui?.max_rows ?? MAX_REPEAT_ROWS,
    MAX_REPEAT_ROWS
  )
  const update = (next: Record<string, string>[]) =>
    onText(JSON.stringify(next))
  return (
    <div className="space-y-2">
      {rows.map((row, index) => (
        <RepeatRow
          key={index}
          row={row}
          index={index}
          columns={columns}
          locked={locked}
          onChange={(next) =>
            update(
              rows.map((item, rowIndex) => (rowIndex === index ? next : item))
            )
          }
          onRemove={() =>
            update(rows.filter((_, rowIndex) => rowIndex !== index))
          }
        />
      ))}
      <RepeatAdd
        count={rows.length}
        max={maxRows}
        locked={locked}
        onAdd={() =>
          update([
            ...rows,
            Object.fromEntries(columns.map((column) => [column.id, ""])),
          ])
        }
      />
    </div>
  )
}

interface RepeatRowProps {
  row: Record<string, string>
  index: number
  columns: QuestionColumn[]
  locked: boolean
  onChange: (row: Record<string, string>) => void
  onRemove: () => void
}

function RepeatRow({
  row,
  index,
  columns,
  locked,
  onChange,
  onRemove,
}: RepeatRowProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <div className="flex min-w-0 items-start gap-2">
      <div className="grid min-w-0 flex-1 grid-cols-1 gap-2 @sm:grid-cols-2">
        {columns.map((column) => (
          <label
            key={column.id}
            className="min-w-0 space-y-1 text-xs text-muted-foreground"
          >
            <span>
              {column.label}
              {column.required && " *"}
            </span>
            <input
              aria-label={t("rowField", {
                index: index + 1,
                label: column.label,
              })}
              disabled={locked}
              value={row[column.id] ?? ""}
              maxLength={MAX_ANSWER_CHARS}
              placeholder={column.placeholder ?? ""}
              className="w-full min-w-0 rounded-md border bg-background px-2.5 py-2 text-xs text-foreground focus-visible:outline-ring"
              onChange={(event) =>
                onChange({ ...row, [column.id]: event.target.value })
              }
            />
          </label>
        ))}
      </div>
      <RepeatRemove index={index} locked={locked} onRemove={onRemove} />
    </div>
  )
}

function RepeatAdd({
  count,
  max,
  locked,
  onAdd,
}: {
  count: number
  max: number
  locked: boolean
  onAdd: () => void
}) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <div className="flex items-center gap-2">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="rounded-md text-xs"
        disabled={locked || count >= max}
        onClick={onAdd}
      >
        <Plus className="size-3.5" />
        {t("addRow")}
      </Button>
      <span className="text-[10px] text-muted-foreground tabular-nums">
        {count} / {max}
      </span>
    </div>
  )
}

function RepeatRemove({
  index,
  locked,
  onRemove,
}: Pick<RepeatRowProps, "index" | "locked" | "onRemove">) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <Button
      type="button"
      size="icon-sm"
      variant="ghost"
      className="mt-5 rounded-md"
      disabled={locked}
      title={t("removeRow", { index: index + 1 })}
      aria-label={t("removeRow", { index: index + 1 })}
      onClick={onRemove}
    >
      <Trash2 className="size-3.5" />
    </Button>
  )
}
