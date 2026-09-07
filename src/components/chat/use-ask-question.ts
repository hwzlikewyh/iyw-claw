import { useRef, useState } from "react"
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
      seed?.[question.id] ?? { chosen: [], otherText: "" },
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
      value && (value.chosen.length > 0 || value.otherText.trim().length > 0)
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
          question.multi_select || !text.trim()
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
  const [error, setError] = useState(false)
  const inFlight = useRef(false)
  const locked = submitting || !!props.readOnly
  const run = async (declined: boolean) => {
    if (inFlight.current || locked) return
    inFlight.current = true
    setSubmitting(true)
    setError(false)
    try {
      const answers = declined
        ? []
        : props.question.questions.map(({ id }) => ({
            questionId: id,
            labels: [
              ...state[id].chosen,
              ...[state[id].otherText.trim()].filter(Boolean),
            ],
          }))
      await props.onAnswer(props.question.question_id, { answers, declined })
    } catch {
      setError(true)
      setSubmitting(false)
    } finally {
      inFlight.current = false
    }
  }
  return { submitting, error, run }
}
