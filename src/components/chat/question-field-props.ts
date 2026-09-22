import type { ReactNode } from "react"
import type { QuestionSpec } from "@/lib/types"
import type { QuestionSelection } from "@/lib/question-answer"

export interface QuestionFieldProps {
  question: QuestionSpec
  value: QuestionSelection
  locked: boolean
  readOnly?: boolean
  onSelect: (label: string) => void
  onText: (text: string) => void
  optionChildren?: (label: string) => ReactNode
}
