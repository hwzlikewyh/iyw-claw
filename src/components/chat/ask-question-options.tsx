import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import { Checkbox } from "@/components/ui/checkbox"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { splitRecommended } from "@/lib/ask-question"
import { cn } from "@/lib/utils"
import type { QuestionOption, QuestionSpec } from "@/lib/types"
import { MAX_ANSWER_CHARS, type QuestionSelection } from "./use-ask-question"

type OptionsProps = {
  question: QuestionSpec
  value: QuestionSelection
  locked: boolean
  readOnly?: boolean
  onSelect: (label: string) => void
  onText: (text: string) => void
}

export function AskQuestionOptions(props: OptionsProps) {
  const { question, value, locked, onSelect } = props
  const rows = question.options.map((option, index) => (
    <OptionRow key={option.label} {...props} option={option} index={index} />
  ))

  return (
    <div className="space-y-2">
      {question.multi_select ? (
        <div className="space-y-2">{rows}</div>
      ) : (
        <RadioGroup
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
}: OptionsProps & { option: QuestionOption; index: number }) {
  const selected = value.chosen.includes(option.label)
  return (
    <Label
      key={option.label}
      className={cn(
        "flex items-start gap-2.5 rounded-lg border p-2.5",
        selected ? "border-primary bg-primary/10" : "border-border/60",
        !locked && "cursor-pointer hover:bg-muted/40"
      )}
    >
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
  return (
    <>
      {" "}
      {(!readOnly || value.otherText) && (
        <label className="block space-y-1 text-xs text-muted-foreground">
          <span>
            {t(question.options.length ? "customAnswer" : "freeText")}
          </span>
          <textarea
            value={value.otherText}
            disabled={locked}
            maxLength={MAX_ANSWER_CHARS}
            rows={3}
            onChange={(event) => onText(event.target.value)}
            placeholder={t("otherPlaceholder")}
            className="w-full resize-y rounded-md border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-ring disabled:opacity-70"
          />
        </label>
      )}
    </>
  )
}
