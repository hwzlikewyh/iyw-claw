import { useTranslations } from "next-intl"
import {
  ArrowRight,
  Check,
  ChevronRight,
  ListChecks,
  MessageCircleQuestionMark,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { hasQuestionAnswer } from "@/lib/question-answer"
import type { QuestionCardViewProps } from "./question-card-content"

export function QuestionHeader({ props, state }: QuestionCardViewProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <header className="flex shrink-0 items-start gap-3 p-4">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-700 dark:text-emerald-400">
        <MessageCircleQuestionMark className="size-4.5" />
      </span>
      <div className="min-w-0 flex-1">
        <h3 className="text-sm font-semibold">
          {state.review ? t("reviewTitle") : (props.title ?? t("title"))}
        </h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {props.subtitle ?? t("subtitle")}
        </p>
      </div>
      {state.questions.length > 1 && (
        <span className="shrink-0 text-[10px] leading-5 text-muted-foreground tabular-nums">
          {state.answered} / {state.questions.length}
        </span>
      )}
    </header>
  )
}

export function QuestionNavigation({ props, state }: QuestionCardViewProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  if (state.questions.length < 2 || state.review || props.readOnly) return null
  return (
    <div className="shrink-0 px-4 pb-3">
      <div className="mb-2 inline-flex rounded-md bg-muted p-0.5">
        {(["form", "steps"] as const).map((layout) => (
          <Button
            type="button"
            key={layout}
            size="xs"
            variant={state.layout === layout ? "secondary" : "ghost"}
            className="rounded-sm text-[11px]"
            aria-pressed={state.layout === layout}
            disabled={state.locked}
            onClick={() => state.setLayout(layout)}
          >
            {layout === "form" ? <ListChecks /> : <ChevronRight />}
            {t(layout === "form" ? "formMode" : "stepsMode")}
          </Button>
        ))}
      </div>
      {state.layout === "steps" && <QuestionSteps state={state} />}
    </div>
  )
}

function QuestionSteps({ state }: Pick<QuestionCardViewProps, "state">) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <nav
      aria-label={t("progress")}
      className="flex gap-1 overflow-x-auto border-b"
    >
      {state.questions.map((question, index) => (
        <Button
          type="button"
          size="sm"
          key={question.id}
          variant="ghost"
          className="rounded-none border-b-2 border-transparent text-xs aria-[current=step]:border-primary aria-[current=step]:text-primary"
          aria-current={state.active === index ? "step" : undefined}
          disabled={state.locked}
          onClick={() => state.setActive(index)}
        >
          {hasQuestionAnswer(question, state.state[question.id]) ? (
            <Check className="size-3" />
          ) : (
            index + 1
          )}
          <span className="max-w-24 truncate">{question.header}</span>
        </Button>
      ))}
    </nav>
  )
}

export function DeferredQuestion({ props, state }: QuestionCardViewProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <section className="mb-2 flex flex-wrap items-center gap-3 rounded-lg border bg-card p-4">
      <MessageCircleQuestionMark className="size-4 text-emerald-600" />
      <div className="min-w-0 flex-1">
        <h3 className="text-sm font-medium">
          {props.title ?? t("savedTitle")}
        </h3>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("savedDescription")}
        </p>
      </div>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="rounded-md text-xs"
        onClick={() => state.setDeferred(false)}
      >
        {t("resume")}
        <ArrowRight className="size-3.5" />
      </Button>
    </section>
  )
}
