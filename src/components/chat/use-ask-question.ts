import { useRef, useState } from "react"
import {
  inputDefaults,
  allowsEmptyInput,
  preservesEmptyText,
  answerError,
} from "@/lib/question-input"
import type {
  PendingQuestionState,
  QuestionAnswer,
  QuestionSpec,
} from "@/lib/types"

export type QuestionSelection = { chosen: string[]; otherText: string }
export type SeedSelections = Record<string, QuestionSelection>
export const MAX_ANSWER_CHARS = 4096

export interface QuestionCardProps {
  question: PendingQuestionState
  onAnswer: (questionId: string, answer: QuestionAnswer) => void | Promise<void>
  readOnly?: boolean
  initialSelections?: SeedSelections
  title?: string
  subtitle?: string
}

function initialState(
  questions: QuestionSpec[],
  seed?: SeedSelections
): SeedSelections {
  return Object.fromEntries(
    questions.map((question) => [
      question.id,
      seed?.[question.id] ?? inputDefaults(question),
    ])
  )
}

export function useAskQuestion(props: QuestionCardProps) {
  const [state, setState] = useState(() =>
    initialState(props.question.questions, props.initialSelections)
  )
  const [active, setActive] = useState(0)
  const submission = useQuestionSubmission(props, state)
  const locked = submission.submitting || !!props.readOnly
  const answered = props.question.questions.filter((question) => {
    const value = state[question.id]
    return (
      question.optional ||
      allowsEmptyInput(question) ||
      (value &&
        (value.chosen.length > 0 ||
          (question.secret || question.input
            ? value.otherText
            : value.otherText.trim()
          ).length > 0))
    )
  }).length
  const select = (question: QuestionSpec, label: string) => {
    if (locked) return
    setState((state) => ({
      ...state,
      [question.id]: selectOption(state[question.id], question, label),
    }))
  }
  const type = (question: QuestionSpec, text: string) => {
    if (locked) return
    setState((state) => ({
      ...state,
      [question.id]: {
        chosen:
          question.multi_select ||
          !(question.secret || question.input ? text : text.trim())
            ? state[question.id].chosen
            : [],
        otherText: text,
      },
    }))
  }

  return {
    state,
    active,
    setActive,
    ...submission,
    locked,
    answered,
    select,
    type,
  }
}

function selectOption(
  current: QuestionSelection,
  question: QuestionSpec,
  label: string
): QuestionSelection {
  const chosen = current.chosen.includes(label)
    ? current.chosen.filter((item) => item !== label)
    : question.multi_select
      ? [...current.chosen, label]
      : [label]
  return { chosen, otherText: question.multi_select ? current.otherText : "" }
}

function useQuestionSubmission(
  props: QuestionCardProps,
  state: SeedSelections
) {
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const inFlight = useRef(false)
  const locked = submitting || !!props.readOnly
  const run = async (declined: boolean) => {
    if (inFlight.current || locked) return
    inFlight.current = true
    setSubmitting(true)
    setError(null)
    try {
      const answers = declined
        ? []
        : props.question.questions.map((question) => {
            const value = state[question.id]
            const text =
              question.secret || question.input
                ? value.otherText
                : value.otherText.trim()
            const includeText =
              text.length > 0 ||
              (value.chosen.length === 0 && preservesEmptyText(question))
            return {
              questionId: question.id,
              labels: [...value.chosen, ...(includeText ? [text] : [])],
            }
          })
      await props.onAnswer(props.question.question_id, { answers, declined })
    } catch (error) {
      setError(
        props.question.questions.some((question) => question.input)
          ? answerError(error, "submitError")
          : "submitError"
      )
      setSubmitting(false)
    } finally {
      inFlight.current = false
    }
  }
  return { submitting, error, run }
}
