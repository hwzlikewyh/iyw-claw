"use client"

import { useId } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { list, record, text, numeric, useViewState } from "./context"

export function Quiz({ props }: { props: Record<string, unknown> }) {
  const t = useTranslations("IntelligentUI")
  const [answers, setAnswers] = useViewState<Record<string, number>>(
    "answers",
    {}
  )
  const [submitted, setSubmitted] = useViewState<boolean>("submitted", false)
  const radioName = useId()
  const questions = list(props.questions).slice(0, 50).map(record)
  return (
    <div className="space-y-4">
      {questions.map((question, index) => (
        <QuizQuestion
          key={index}
          question={question}
          index={index}
          name={radioName}
          answer={answers[index]}
          submitted={submitted}
          choose={(value) => {
            setAnswers({ ...answers, [index]: value })
            setSubmitted(false)
          }}
        />
      ))}
      <Button
        onClick={() => setSubmitted(true)}
        disabled={
          !questions.length || Object.keys(answers).length < questions.length
        }
      >
        {t("checkAnswers")}
      </Button>
      {submitted && (
        <p>
          {questions.filter((q, i) => q.answer === answers[i]).length} /{" "}
          {questions.length}
        </p>
      )}
    </div>
  )
}
export function DecisionTree({ props }: { props: Record<string, unknown> }) {
  const [path, setPath] = useViewState<number[]>("path", [])
  let node = props
  for (const index of path) {
    const option = record(list(node.options)[index])
    node = option.next
      ? record(option.next)
      : { result: option.result ?? option.label }
  }
  return (
    <div className="space-y-3">
      <p className="font-medium">{text(node.question ?? node.result ?? "")}</p>
      {list(node.options)
        .slice(0, 20)
        .map((option, i) => {
          const p = record(option)
          return (
            <div key={i}>
              <Button variant="outline" onClick={() => setPath([...path, i])}>
                {text(p.label)}
              </Button>
              {!p.next && Boolean(p.result) && (
                <p className="mt-1 text-xs">{text(p.result)}</p>
              )}
            </div>
          )
        })}
      {path.length > 0 && (
        <Button variant="ghost" onClick={() => setPath(path.slice(0, -1))}>
          ←
        </Button>
      )}
    </div>
  )
}

function QuizQuestion({
  question: q,
  index,
  name,
  answer,
  submitted,
  choose,
}: {
  question: Record<string, unknown>
  index: number
  name: string
  answer?: number
  submitted: boolean
  choose: (value: number) => void
}) {
  return (
    <fieldset className="space-y-2">
      <legend className="font-medium">{text(q.question)}</legend>
      {list(q.options).map((option, j) => (
        <label key={j} className="flex items-center gap-2 text-sm">
          <input
            type="radio"
            name={`${name}-${index}`}
            checked={answer === j}
            onChange={() => choose(j)}
          />
          {text(option)}
        </label>
      ))}
      {submitted && (
        <p
          className={
            answer === q.answer ? "text-green-600" : "text-destructive"
          }
        >
          {answer === q.answer ? "✓" : "✗"}{" "}
          {text(q.explanation ?? list(q.options)[numeric(q.answer)])}
        </p>
      )}
    </fieldset>
  )
}
