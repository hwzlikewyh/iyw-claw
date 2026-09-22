import { useTranslations } from "next-intl"
import { Fragment } from "react"
import { Pencil } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { QuestionSpec } from "@/lib/types"
import {
  answerLabels,
  questionControl,
  repeatedRows,
  type QuestionSelection,
} from "@/lib/question-answer"
import { AskQuestionOptions } from "./ask-question-options"
import type { QuestionCardProps, useAskQuestion } from "./use-ask-question"

export type QuestionCardState = ReturnType<typeof useAskQuestion>
export interface QuestionCardViewProps {
  props: QuestionCardProps
  state: QuestionCardState
}
const COMPACT_CONTROLS = [
  "text",
  "number",
  "date",
  "select",
  "combobox",
  "password",
]

function inlineParent(question: QuestionSpec, questions: QuestionSpec[]) {
  const when = question.ui?.when
  if (!when) return undefined
  const parent = questions.find((item) => item.id === when.question_id)
  return parent &&
    ["radio", "checkbox"].includes(questionControl(parent)) &&
    parent.options.some((option) => option.label === when.equals)
    ? parent
    : undefined
}

export function QuestionCardContent({ props, state }: QuestionCardViewProps) {
  if (state.review || props.readOnly)
    return <QuestionReview props={props} state={state} />
  const roots = state.questions.filter(
    (question) => !inlineParent(question, state.questions)
  )
  const current = state.questions[state.active]
  let parent = current
  while (parent && inlineParent(parent, state.questions))
    parent = inlineParent(parent, state.questions)!
  const shown =
    state.layout === "form"
      ? roots
      : roots.filter((question) => question.id === parent?.id)
  return (
    <div
      className={
        state.layout === "form"
          ? "grid grid-cols-1 gap-4 @lg:grid-cols-2"
          : "space-y-4"
      }
    >
      {shown.map((question, index) => (
        <Fragment key={question.id}>
          {question.ui?.group &&
            (index === 0 ||
              shown[index - 1].ui?.group !== question.ui.group) && (
              <h4 className="col-span-full border-t border-border/60 pt-3 text-xs font-semibold">
                {question.ui.group}
              </h4>
            )}
          <div
            className={
              state.layout === "form" &&
              COMPACT_CONTROLS.includes(questionControl(question))
                ? "min-w-0"
                : "col-span-full min-w-0"
            }
          >
            <QuestionField question={question} view={{ props, state }} />
          </div>
        </Fragment>
      ))}
    </div>
  )
}

function QuestionField({
  question,
  view,
}: {
  question: QuestionSpec
  view: QuestionCardViewProps
}) {
  const t = useTranslations("Folder.chat.askQuestion")
  const { state, props } = view
  const error = state.fieldErrors[question.id]
  return (
    <div
      data-question-id={question.id}
      className="min-w-0 space-y-2 [overflow-wrap:anywhere]"
      role="group"
      aria-label={question.header}
    >
      <div className="flex items-start gap-2">
        <p className="text-sm font-medium whitespace-pre-wrap">
          {question.question}
        </p>
        <span className="shrink-0 text-[10px] leading-5 text-muted-foreground">
          {t(question.optional ? "optional" : "required")}
        </span>
      </div>
      <AskQuestionOptions
        question={question}
        value={state.state[question.id]}
        locked={state.locked}
        readOnly={props.readOnly}
        onSelect={(label) => state.select(question, label)}
        onText={(text) => state.type(question, text)}
        optionChildren={(label) => (
          <OptionFields question={question} view={view} label={label} />
        )}
      />
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {t(error)}
        </p>
      )}
    </div>
  )
}

function OptionFields({
  question,
  view,
  label,
}: {
  question: QuestionSpec
  view: QuestionCardViewProps
  label: string
}) {
  const children = view.state.questions.filter(
    (child) =>
      child.ui?.when?.question_id === question.id &&
      child.ui.when.equals === label
  )
  return children.length ? (
    <div className="space-y-3 px-3 pb-3 ps-8">
      {children.map((child) => (
        <QuestionField key={child.id} question={child} view={view} />
      ))}
    </div>
  ) : null
}

function QuestionReview({ props, state }: QuestionCardViewProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <div className="divide-y divide-border/60">
      {state.questions.map((question, index) => {
        const text = question.secret
          ? t("secretValue")
          : reviewText(question, state.state[question.id])
        return (
          <div
            key={question.id}
            className="flex items-start gap-3 py-3 first:pt-0"
          >
            <div className="min-w-0 flex-1 space-y-1">
              <p className="text-xs text-muted-foreground">{question.header}</p>
              <p className="text-sm whitespace-pre-wrap [overflow-wrap:anywhere]">
                {text || t("notAnswered")}
              </p>
            </div>
            {!props.readOnly && (
              <Button
                type="button"
                size="icon-xs"
                variant="ghost"
                disabled={state.locked}
                title={t("editAnswer")}
                aria-label={t("editAnswer")}
                onClick={() => {
                  state.setActive(index)
                  state.setReview(false)
                }}
              >
                <Pencil className="size-3.5" />
              </Button>
            )}
          </div>
        )
      })}
    </div>
  )
}

function reviewText(question: QuestionSpec, value: QuestionSelection) {
  const labels = answerLabels(question, value)
  if (questionControl(question) !== "repeat") return labels.join("; ")
  return repeatedRows(labels[0] ?? "")
    .map((row) =>
      question.ui?.columns
        ?.map((column) => `${column.label}: ${row[column.id] ?? ""}`)
        .join("; ")
    )
    .join("\n")
}
