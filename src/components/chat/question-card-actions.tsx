import { useTranslations } from "next-intl"
import { ArrowLeft, ArrowRight, Check, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { QuestionCardViewProps } from "./question-card-content"

function advance({ state }: QuestionCardViewProps) {
  if (state.review) {
    void state.run(false)
    return
  }
  if (state.layout === "steps" && state.active < state.questions.length - 1) {
    const current = state.questions[state.active]
    const children = state.questions.filter(
      (question) => question.ui?.when?.question_id === current.id
    )
    if (state.validate([current, ...children]))
      state.setActive(state.active + 1)
    return
  }
  if (!state.validate()) return
  if (state.questions.length === 1) void state.run(false)
  else state.setReview(true)
}

export function QuestionFooter(view: QuestionCardViewProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  const { state } = view
  return (
    <footer className="shrink-0 border-t border-border/60 p-4">
      {state.error && (
        <p role="alert" className="mb-3 text-xs text-destructive">
          {state.error === "submitError" ? t("submitError") : state.error}
        </p>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="rounded-md text-xs text-muted-foreground"
          disabled={state.locked}
          onClick={() => state.setDeferred(true)}
        >
          {t("later")}
        </Button>
        <div className="ms-auto flex max-w-full flex-wrap justify-end gap-2">
          {view.props.allowSkip !== false && (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="rounded-md text-xs text-muted-foreground"
              disabled={state.locked}
              onClick={() => void state.run(true)}
            >
              {t("skip")}
            </Button>
          )}
          <QuestionBack {...view} />
          <QuestionSubmit {...view} />
        </div>
      </div>
    </footer>
  )
}

function QuestionBack({ state }: QuestionCardViewProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  if (!state.review && (state.layout !== "steps" || state.active === 0))
    return null
  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      className="rounded-md text-xs"
      disabled={state.locked}
      onClick={() =>
        state.review
          ? state.setReview(false)
          : state.setActive(state.active - 1)
      }
    >
      <ArrowLeft className="size-3.5" />
      {t(state.review ? "backToEdit" : "previous")}
    </Button>
  )
}

function QuestionSubmit(view: QuestionCardViewProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  const { state } = view
  const next =
    !state.review &&
    state.layout === "steps" &&
    state.active < state.questions.length - 1
  const label =
    state.review || state.questions.length === 1
      ? "submit"
      : next
        ? "next"
        : "review"
  const Icon = state.submitting
    ? Loader2
    : label === "submit"
      ? Check
      : ArrowRight
  return (
    <Button
      type="button"
      size="sm"
      className="rounded-md text-xs"
      disabled={state.locked}
      onClick={() => advance(view)}
      data-question-submit
    >
      <Icon className={`size-3.5 ${state.submitting ? "animate-spin" : ""}`} />
      {t(label)}
    </Button>
  )
}
