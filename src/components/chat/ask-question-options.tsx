import { useTranslations } from "next-intl"
import { useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import { Checkbox } from "@/components/ui/checkbox"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { splitRecommended } from "@/lib/ask-question"
import { cn } from "@/lib/utils"
import type { QuestionOption } from "@/lib/types"
import { questionControl } from "@/lib/question-answer"
import { QuestionSelectField } from "./question-select-field"
import { QuestionTextField } from "./question-text-field"
import { QuestionRepeatField } from "./question-repeat-field"
import type { QuestionFieldProps as OptionsProps } from "./question-field-props"

export function AskQuestionOptions(props: OptionsProps) {
  const { question, value, locked, onSelect } = props
  const control = questionControl(question)
  if (control === "repeat") return <QuestionRepeatField {...props} />
  if (
    ["text", "textarea", "date", "number", "password", "switch"].includes(
      control
    )
  )
    return <QuestionTextField {...props} />
  if (control === "select" || control === "combobox")
    return (
      <div className="space-y-2">
        <QuestionSelectField {...props} />
        <FreeTextAnswer {...props} />
      </div>
    )
  const rows = question.options.map((option, index) => (
    <OptionRow key={option.label} {...props} option={option} index={index} />
  ))

  return (
    <div className="space-y-2">
      {question.multi_select ? (
        <div className="space-y-2">{rows}</div>
      ) : (
        <RadioGroup
          aria-label={question.header}
          disabled={locked}
          value={String(
            question.options.findIndex((option) =>
              value.chosen.includes(option.label)
            )
          )}
          onValueChange={(index) => {
            const option = question.options[Number(index)]
            if (option) onSelect(option.label)
          }}
        >
          {rows}
        </RadioGroup>
      )}
      <FreeTextAnswer {...props} />
    </div>
  )
}

function OptionBody({ option }: { option: QuestionOption }) {
  const t = useTranslations("Folder.chat.askQuestion")
  const { text, recommended } = splitRecommended(option.label)
  return (
    <span className="min-w-0 flex-1">
      <span className="flex flex-wrap items-center gap-1.5 text-sm font-medium">
        {text}
        {recommended && <Badge variant="secondary">{t("recommended")}</Badge>}
      </span>
      {option.description && (
        <span className="mt-1 block text-xs text-muted-foreground">
          {option.description}
        </span>
      )}
    </span>
  )
}

function OptionRow({
  question,
  value,
  locked,
  onSelect,
  option,
  index,
  optionChildren,
}: OptionsProps & { option: QuestionOption; index: number }) {
  const selected = value.chosen.includes(option.label)
  return (
    <div
      key={option.label}
      className={cn(
        "rounded-md border",
        selected ? "border-primary bg-primary/10" : "border-border/60",
        !locked && "cursor-pointer hover:bg-muted/40"
      )}
    >
      <Label className="flex cursor-pointer items-start gap-2.5 p-2.5">
        {question.multi_select ? (
          <Checkbox
            checked={selected}
            disabled={locked}
            onCheckedChange={() => onSelect(option.label)}
          />
        ) : (
          <RadioGroupItem value={String(index)} disabled={locked} />
        )}
        <OptionBody option={option} />
      </Label>
      {selected && optionChildren?.(option.label)}
    </div>
  )
}

function FreeTextAnswer({
  question,
  value,
  locked,
  readOnly,
  onText,
}: OptionsProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  const [expanded, setExpanded] = useState(Boolean(value.otherText))
  if (question.input?.allow_other === false) return null
  return (
    <>
      {" "}
      {(!readOnly || value.otherText) && (
        <div className="block space-y-1 text-xs text-muted-foreground">
          <button
            type="button"
            disabled={locked}
            aria-expanded={expanded || Boolean(value.otherText)}
            className="text-xs text-muted-foreground hover:text-foreground"
            onClick={() => setExpanded(!expanded)}
          >
            {t("customAnswer")}
          </button>
          {(expanded || Boolean(value.otherText)) && (
            <QuestionTextField
              question={{
                ...question,
                options: [],
                ui: { ...question.ui, control: "textarea" },
              }}
              value={value}
              locked={locked}
              readOnly={readOnly}
              onSelect={() => {}}
              onText={onText}
            />
          )}
        </div>
      )}
    </>
  )
}
