export type QuestionControl =
  | "text"
  | "textarea"
  | "select"
  | "combobox"
  | "radio"
  | "checkbox"
  | "number"
  | "date"
  | "switch"
  | "password"
  | "repeat"

export interface QuestionColumn {
  id: string
  label: string
  required?: boolean
  placeholder?: string | null
}

export interface QuestionUi {
  not_before?: string | null
  control?: QuestionControl | null
  layout?: "form" | "steps" | null
  group?: string | null
  placeholder?: string | null
  when?: { question_id: string; equals: string } | null
  columns?: QuestionColumn[]
  max_rows?: number | null
}
