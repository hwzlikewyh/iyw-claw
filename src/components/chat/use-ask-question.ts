import { useRef, useState } from "react"
import { inputDefaults, answerError } from "@/lib/question-input"
import type {
  PendingQuestionState,
  QuestionAnswer,
  QuestionSpec,
} from "@/lib/types"

import {
  buildQuestionAnswers,
  hasQuestionAnswer,
  questionAnswerError,
  questionDateError,
  visibleQuestions,
  type QuestionSelection,
  type SeedSelections,
  type QuestionErrorKey,
} from "@/lib/question-answer"
export { MAX_ANSWER_CHARS } from "@/lib/question-answer"
export type { QuestionSelection, SeedSelections } from "@/lib/question-answer"

export interface QuestionCardProps {
  question: PendingQuestionState
  onAnswer: (questionId: string, answer: QuestionAnswer) => void | Promise<void>
  readOnly?: boolean
  allowSkip?: boolean
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
  const selection = useQuestionSelection(props)
  const { state, questions, setActive, setFieldErrors } = selection
  const [review, setReview] = useState(false)
  const [deferred, setDeferred] = useState(false)
  const [layout, setLayout] = useState(() => initialLayout(props))
  const submission = useQuestionSubmission(props, state)
  const locked = submission.submitting || !!props.readOnly
  const answered = questions.filter((question) =>
    hasQuestionAnswer(question, state[question.id])
  ).length
  const validate = (items = questions) => {
    const errors = Object.fromEntries(
      items.flatMap((question) => {
        const error =
          questionAnswerError(question, state[question.id]) ||
          (questions.some((item) => item.id === question.ui?.not_before)
            ? questionDateError(question, state)
            : null)
        return error ? [[question.id, error]] : []
      })
    )
    setFieldErrors(errors)
    const first = questions.findIndex((question) => errors[question.id])
    if (first >= 0) setActive(first)
    return first < 0
  }
  return {
    ...selection,
    layout,
    setLayout,
    review,
    setReview,
    deferred,
    setDeferred,
    validate,
    ...submission,
    locked,
    answered,
    select: (question: QuestionSpec, label: string) => {
      if (!locked) selection.select(question, label)
    },
    type: (question: QuestionSpec, text: string) => {
      if (!locked) selection.type(question, text)
    },
  }
}

function initialLayout(props: QuestionCardProps): "form" | "steps" {
  return (
    props.question.questions[0]?.ui?.layout ??
    (props.question.questions.some((question) => question.input || question.ui)
      ? "form"
      : "steps")
  )
}

function useQuestionSelection(props: QuestionCardProps) {
  const [state, setState] = useState(() =>
    initialState(props.question.questions, props.initialSelections)
  )
  const [active, setActive] = useState(0)
  const [fieldErrors, setFieldErrors] = useState<
    Record<string, QuestionErrorKey | "">
  >({})
  const questions = visibleQuestions(props.question.questions, state)
  const clearError = (id: string) =>
    setFieldErrors((errors) => ({ ...errors, [id]: "" }))
  const select = (question: QuestionSpec, label: string) => {
    clearError(question.id)
    setState((state) => ({
      ...state,
      [question.id]: selectOption(state[question.id], question, label),
    }))
  }
  const type = (question: QuestionSpec, text: string) => {
    clearError(question.id)
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
    questions,
    active: Math.min(active, Math.max(questions.length - 1, 0)),
    setActive,
    fieldErrors,
    setFieldErrors,
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
        : buildQuestionAnswers(props.question.questions, state)
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
