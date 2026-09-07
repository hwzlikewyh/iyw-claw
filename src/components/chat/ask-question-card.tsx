"use client"

import { useTranslations } from "next-intl"
import {
  Check,
  ChevronLeft,
  ChevronRight,
  Loader2,
  MessageCircleQuestionMark,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { AskQuestionOptions } from "./ask-question-options"
import { useAskQuestion, type QuestionCardProps } from "./use-ask-question"

export function AskQuestionCard(props: QuestionCardProps) {
  return <QuestionCard key={props.question.question_id} {...props} />
}

function QuestionCard(props: QuestionCardProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  const state = useAskQuestion(props)
  const questions = props.question.questions
  const current = questions[state.active]
  if (!current) return null
  return (
    <section
      aria-label={props.title ?? t("title")}
      className="mb-2 flex max-h-[70svh] flex-col overflow-hidden rounded-xl border border-primary/30 bg-card"
    >
      {questions.length > 1 && (
        <Progress
          className="h-1 shrink-0 rounded-none"
          value={(state.answered / questions.length) * 100}
          aria-label={t("title")}
        />
      )}
      <QuestionHeader props={props} state={state} />
      <QuestionNavigation questions={questions} state={state} />
      <div className="min-h-0 space-y-3 overflow-auto px-3 pb-3">
        <p className="whitespace-pre-wrap text-sm">{current.question}</p>
        <AskQuestionOptions
          question={current}
          value={state.state[current.id]}
          locked={state.locked}
          readOnly={props.readOnly}
          onSelect={(label) => state.select(current, label)}
          onText={(text) => state.type(current, text)}
        />
      </div>
      {!props.readOnly && (
        <QuestionFooter count={questions.length} state={state} />
      )}
    </section>
  )
}

type CardState = ReturnType<typeof useAskQuestion>

function QuestionNavigation({
  questions,
  state,
}: {
  questions: QuestionCardProps["question"]["questions"]
  state: CardState
}) {
  if (questions.length < 2) return null
  return (
    <nav className="flex shrink-0 gap-1 overflow-auto px-3 pb-3">
      {questions.map((question, index) => {
        const value = state.state[question.id]
        return (
          <Button
            key={question.id}
            size="sm"
            variant={index === state.active ? "secondary" : "ghost"}
            disabled={state.submitting}
            aria-current={index === state.active ? "step" : undefined}
            onClick={() => state.setActive(index)}
          >
            {value.chosen.length > 0 || value.otherText.trim() ? (
              <Check className="size-3.5" />
            ) : (
              index + 1
            )}
            <span className="max-w-28 truncate">{question.header}</span>
          </Button>
        )
      })}
    </nav>
  )
}

function QuestionFooter({ count, state }: { count: number; state: CardState }) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-t p-3">
      <Button
        variant="ghost"
        size="sm"
        disabled={state.submitting}
        onClick={() => void state.run(true)}
      >
        {t("skip")}
      </Button>
      {state.error && (
        <span role="alert" className="text-xs text-destructive">
          {t("submitError")}
        </span>
      )}
      <div className="ml-auto flex items-center gap-2">
        <QuestionSteps count={count} state={state} />
        <Button
          size="sm"
          disabled={state.submitting || state.answered !== count}
          onClick={() => void state.run(false)}
        >
          {state.submitting && <Loader2 className="size-3.5 animate-spin" />}
          {t("submit")}
        </Button>
      </div>
    </div>
  )
}

function QuestionHeader({
  props,
  state,
}: {
  props: QuestionCardProps
  state: CardState
}) {
  const t = useTranslations("Folder.chat.askQuestion")
  const questions = props.question.questions
  return (
    <div className="flex shrink-0 items-center gap-2.5 p-3">
      <MessageCircleQuestionMark className="size-5 text-primary" />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">{props.title ?? t("title")}</p>
        {(props.subtitle ?? t("subtitle")) && (
          <p className="text-xs text-muted-foreground">
            {props.subtitle ?? t("subtitle")}
          </p>
        )}
      </div>
      {questions.length > 1 && (
        <span className="text-xs text-muted-foreground">
          {state.answered}/{questions.length}
        </span>
      )}
    </div>
  )
}

function QuestionSteps({ count, state }: { count: number; state: CardState }) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <>
      {" "}
      {state.active > 0 && (
        <Button
          variant="outline"
          size="sm"
          disabled={state.submitting}
          onClick={() => state.setActive(state.active - 1)}
        >
          <ChevronLeft className="size-3.5" />
          {t("previous")}
        </Button>
      )}
      {state.active < count - 1 && (
        <Button
          variant="outline"
          size="sm"
          disabled={state.submitting}
          onClick={() => state.setActive(state.active + 1)}
        >
          {t("next")}
          <ChevronRight className="size-3.5" />
        </Button>
      )}
    </>
  )
}
