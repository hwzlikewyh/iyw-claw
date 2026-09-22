import type { QuestionInputSpec } from "./types"
import type { QuestionUi } from "./question-ui"

const CONTROLS = new Set([
  "text",
  "textarea",
  "select",
  "combobox",
  "radio",
  "checkbox",
  "number",
  "date",
  "switch",
  "password",
  "repeat",
])
const INPUT_TYPES = new Set(["string", "number", "integer", "boolean", "array"])
function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined
}

export function questionHistoryMetadata(raw: Record<string, unknown>) {
  if (
    !raw.ui &&
    !raw.input &&
    raw.secret === undefined &&
    raw.optional === undefined
  )
    return {}
  const source = record(raw.ui)
  const ui: QuestionUi = {}
  if (typeof source?.control === "string" && CONTROLS.has(source.control))
    ui.control = source.control as QuestionUi["control"]
  if (source?.layout === "form" || source?.layout === "steps")
    ui.layout = source.layout
  for (const key of ["group", "placeholder"] as const)
    if (typeof source?.[key] === "string") ui[key] = source[key]
  if (Array.isArray(source?.columns))
    ui.columns = source.columns.flatMap((item) => {
      const column = record(item)
      return column &&
        typeof column.id === "string" &&
        typeof column.label === "string"
        ? [
            {
              id: column.id,
              label: column.label,
              required: column.required === true,
            },
          ]
        : []
    })
  const property = record(raw.input)
  const schema = record(property?.schema) ?? property
  const input =
    schema && INPUT_TYPES.has(String(schema.type))
      ? {
          schema: schema as unknown as QuestionInputSpec["schema"],
          values: (record(property?.values) as Record<string, string>) ?? {},
          default_values: [],
          allow_other: true,
        }
      : undefined
  return {
    secret: raw.secret === true || ui.control === "password",
    optional: raw.optional === true,
    ui,
    input,
  }
}
