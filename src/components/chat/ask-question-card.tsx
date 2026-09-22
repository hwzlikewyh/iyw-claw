"use client"

import { useEffect, useRef } from "react"
import { useAskQuestion, type QuestionCardProps } from "./use-ask-question"
import { QuestionCardContent } from "./question-card-content"
import {
  DeferredQuestion,
  QuestionHeader,
  QuestionNavigation,
} from "./question-card-chrome"
import { QuestionFooter } from "./question-card-actions"

export function AskQuestionCard(props: QuestionCardProps) {
  return <QuestionCard key={props.question.question_id} {...props} />
}

function QuestionCard(props: QuestionCardProps) {
  const state = useAskQuestion(props)
  const container = useRef<HTMLElement>(null)
  const errorId = Object.keys(state.fieldErrors).find(
    (id) => state.fieldErrors[id]
  )
  const scrollId = errorId ?? state.questions[state.active]?.id
  useEffect(() => {
    const id = scrollId
    if (state.review || !id) return
    const field = Array.from(
      container.current?.querySelectorAll<HTMLElement>("[data-question-id]") ??
        []
    ).find((element) => element.dataset.questionId === id)
    if (errorId)
      field
        ?.querySelector<HTMLElement>("input,textarea,button")
        ?.focus({ preventScroll: true })
    if (errorId || state.layout === "steps")
      field?.scrollIntoView({ block: "nearest" })
  }, [scrollId, errorId, state.layout, state.review])
  if (!state.questions.length) return null
  if (state.deferred && !props.readOnly)
    return <DeferredQuestion props={props} state={state} />
  return (
    <section
      ref={container}
      aria-label={props.title}
      className="@container mb-2 flex max-h-[70svh] min-w-0 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-sm"
    >
      <QuestionHeader props={props} state={state} />
      <QuestionNavigation props={props} state={state} />
      <div className="min-h-0 min-w-0 overflow-y-auto overscroll-contain px-4 pb-4">
        <QuestionCardContent props={props} state={state} />
      </div>
      {!props.readOnly && <QuestionFooter props={props} state={state} />}
    </section>
  )
}
