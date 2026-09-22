import type { QuestionAnswerItem, QuestionSpec } from "@/lib/types"
import { allowsEmptyInput, preservesEmptyText } from "./question-input"

export const MAX_ANSWER_CHARS = 4096
export const MAX_REPEAT_ROWS = 20
const SEARCH_OPTION_COUNT = 7
export type QuestionSelection = {
  chosen: string[]
  otherText: string
  otherActive?: boolean
}
export type SeedSelections = Record<string, QuestionSelection>
export type QuestionErrorKey =
  | "dateOrder"
  | "requiredError"
  | "invalidInput"
  | "tooLong"
  | "tooManyRows"
  | "rowRequired"

export function questionControl(question: QuestionSpec) {
  if (question.secret) return "password"
  if (question.ui?.control) return question.ui.control
  const schema = question.input?.schema
  if (schema?.type === "boolean") return "switch"
  if (question.multi_select && question.options.length) return "checkbox"
  if (question.options.length >= SEARCH_OPTION_COUNT) return "combobox"
  if (question.options.length) return "radio"
  if (schema?.type === "number" || schema?.type === "integer") return "number"
  if (schema?.format === "date") return "date"
  return "textarea"
}

export function repeatedRows(text: string): Record<string, string>[] {
  if (!text) return []
  try {
    const value: unknown = JSON.parse(text)
    if (!Array.isArray(value)) return []
    return value.filter(
      (row): row is Record<string, string> =>
        Boolean(row) &&
        typeof row === "object" &&
        !Array.isArray(row) &&
        Object.values(row).every((value) => typeof value === "string")
    )
  } catch {
    return []
  }
}

export function answerLabels(
  question: QuestionSpec,
  value: QuestionSelection
): string[] {
  if (questionControl(question) === "repeat") {
    const rows = repeatedRows(value.otherText).filter((row) =>
      Object.values(row).some((text) => text.trim())
    )
    return rows.length || !question.optional ? [JSON.stringify(rows)] : []
  }
  const text =
    question.secret || question.input ? value.otherText : value.otherText.trim()
  const include =
    text.length > 0 || (!value.chosen.length && preservesEmptyText(question))
  return [...value.chosen, ...(include ? [text] : [])]
}

export function visibleQuestions(
  questions: QuestionSpec[],
  state: SeedSelections
) {
  const visible = new Set<string>()
  return questions.filter((question) => {
    const when = question.ui?.when
    const parent =
      when && questions.find((item) => item.id === when.question_id)
    const shown =
      !when ||
      Boolean(
        parent &&
        visible.has(parent.id) &&
        answerLabels(parent, state[parent.id]).includes(when.equals)
      )
    if (shown) visible.add(question.id)
    return shown
  })
}

export function hasQuestionAnswer(
  question: QuestionSpec,
  value: QuestionSelection
) {
  return answerLabels(question, value).some((text) =>
    questionControl(question) === "repeat"
      ? repeatedRows(text).length > 0
      : text.length > 0
  )
}

export function buildQuestionAnswers(
  questions: QuestionSpec[],
  state: SeedSelections
): QuestionAnswerItem[] {
  return visibleQuestions(questions, state).map((question) => ({
    questionId: question.id,
    labels: answerLabels(question, state[question.id]),
  }))
}

export function questionDateError(
  question: QuestionSpec,
  state: SeedSelections
): QuestionErrorKey | null {
  const parent = question.ui?.not_before
  const minimum = parent ? state[parent]?.otherText : ""
  const value = state[question.id]?.otherText
  return minimum && value && value < minimum ? "dateOrder" : null
}

export function questionAnswerError(
  question: QuestionSpec,
  value: QuestionSelection
): QuestionErrorKey | null {
  const labels = answerLabels(question, value)
  if (labels.some((label) => Array.from(label).length > MAX_ANSWER_CHARS))
    return "tooLong"
  if (
    !hasQuestionAnswer(question, value) &&
    !question.optional &&
    !allowsEmptyInput(question)
  )
    return "requiredError"
  if (
    !labels.length ||
    (question.optional && !hasQuestionAnswer(question, value))
  )
    return null
  if (questionControl(question) === "repeat")
    return repeatedAnswerError(question, value)
  const schema = question.input?.schema
  if (!schema) return null
  const values = labels.map((label) => question.input?.values[label] ?? label)
  if (
    question.input?.allow_other === false &&
    labels.some(
      (label) => !question.options.some((option) => option.label === label)
    )
  )
    return "invalidInput"
  if (schema.type === "array")
    return countError(values.length, schema.minItems, schema.maxItems)
  if (values.length !== 1) return "invalidInput"
  if (schema.type === "number" || schema.type === "integer") {
    const number = Number(values[0])
    if (
      !values[0].trim() ||
      !Number.isFinite(number) ||
      (schema.type === "integer" && !Number.isSafeInteger(number))
    )
      return "invalidInput"
    return countError(number, schema.minimum, schema.maximum)
  }
  if (schema.type === "boolean")
    return ["true", "false"].includes(values[0]) ? null : "invalidInput"
  return stringAnswerError(question, values[0])
}

function countError(value: number, min?: number, max?: number) {
  return (min !== undefined && value < min) ||
    (max !== undefined && value > max)
    ? "invalidInput"
    : null
}

function stringAnswerError(question: QuestionSpec, text: string) {
  const schema = question.input!.schema
  const lengthError = countError(
    Array.from(text).length,
    schema.minLength,
    schema.maxLength
  )
  if (lengthError) return lengthError
  if (
    schema.format === "date" &&
    (!/^\d{4}-\d{2}-\d{2}$/.test(text) ||
      Number.isNaN(Date.parse(text)) ||
      new Date(text).toISOString().slice(0, 10) !== text)
  )
    return "invalidInput"
  if (schema.format === "email" && !/^[^\s@]+@[^\s@]+$/.test(text))
    return "invalidInput"
  if (schema.format === "uri") {
    try {
      new URL(text)
    } catch {
      return "invalidInput"
    }
  }
  if (schema.format === "date-time" && Number.isNaN(Date.parse(text)))
    return "invalidInput"
  // 正则由后端统一验证，避免不同正则引擎或不可信表达式阻塞界面。
  return null
}

function repeatedAnswerError(question: QuestionSpec, value: QuestionSelection) {
  const rows = repeatedRows(value.otherText).filter((row) =>
    Object.values(row).some((text) => text.trim())
  )
  if (!question.optional && !rows.length) return "requiredError"
  if (rows.length > (question.ui?.max_rows ?? MAX_REPEAT_ROWS))
    return "tooManyRows"
  return rows.some((row) =>
    question.ui?.columns?.some(
      (column) => column.required && !row[column.id]?.trim()
    )
  )
    ? "rowRequired"
    : null
}
