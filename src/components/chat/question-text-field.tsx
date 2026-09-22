import { useState } from "react"
import { useTranslations } from "next-intl"
import { Eye, EyeOff } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { MAX_ANSWER_CHARS, questionControl } from "@/lib/question-answer"
import type { QuestionFieldProps } from "./question-field-props"

const inputClass =
  "w-full min-w-0 rounded-md border border-input bg-background px-3 py-2 text-xs text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30 disabled:opacity-70"

export function QuestionTextField(props: QuestionFieldProps) {
  return questionControl(props.question) === "switch" ? (
    <QuestionSwitch {...props} />
  ) : (
    <TextInput {...props} />
  )
}

function QuestionSwitch({
  question,
  value,
  locked,
  onSelect,
  onText,
}: QuestionFieldProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  const mapped = (text: string) =>
    question.options.find(
      (option) =>
        (question.input?.values[option.label] ?? option.label) === text
    )?.label
  const yes = mapped("true")
  const checked = yes ? value.chosen.includes(yes) : value.otherText === "true"
  return (
    <div className="flex items-center gap-3">
      <Switch
        aria-label={question.header}
        checked={checked}
        disabled={locked}
        onCheckedChange={(next) => {
          const label = mapped(String(next))
          if (label) onSelect(label)
          else onText(String(next))
        }}
      />
      <span className="text-xs text-muted-foreground">
        {value.chosen.length || value.otherText
          ? t(checked ? "yes" : "no")
          : t("notAnswered")}
      </span>
      {!value.chosen.length && !value.otherText && (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          disabled={locked}
          onClick={() => {
            const label = mapped("false")
            if (label) onSelect(label)
            else onText("false")
          }}
        >
          {t("no")}
        </Button>
      )}
    </div>
  )
}

function TextInput(props: QuestionFieldProps) {
  const { question, value, locked, readOnly, onText } = props
  const t = useTranslations("Folder.chat.askQuestion")
  const [revealed, setRevealed] = useState(false)
  const control = questionControl(question)
  const schema = question.input?.schema
  const type = inputType(question, revealed)
  const common = {
    "aria-label": question.header,
    "aria-required": !question.optional,
    value: readOnly && question.secret ? t("secretValue") : value.otherText,
    disabled: locked,
    placeholder: question.ui?.placeholder ?? t("otherPlaceholder"),
    maxLength: Math.min(
      schema?.maxLength ?? MAX_ANSWER_CHARS,
      MAX_ANSWER_CHARS
    ),
    onChange: (
      event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>
    ) => onText(event.target.value),
    className: inputClass,
  }
  if (control === "textarea")
    return (
      <textarea {...common} rows={3} className={`${inputClass} resize-y`} />
    )
  return (
    <div className="relative">
      <input
        {...common}
        type={type}
        min={schema?.minimum}
        max={schema?.maximum}
        step={schema?.type === "integer" ? 1 : "any"}
        autoComplete="off"
        className={`${inputClass} dark:[color-scheme:dark] ${question.secret ? "pe-10" : ""}`}
      />
      {question.secret && !readOnly && (
        <SecretToggle
          revealed={revealed}
          locked={locked}
          toggle={() => setRevealed(!revealed)}
        />
      )}
    </div>
  )
}

function inputType(
  question: QuestionFieldProps["question"],
  revealed: boolean
) {
  const control = questionControl(question)
  if (control === "password") return revealed ? "text" : "password"
  if (control === "number" || control === "date") return control
  return question.input?.schema.format === "email" ? "email" : "text"
}

function SecretToggle({
  revealed,
  locked,
  toggle,
}: {
  revealed: boolean
  locked: boolean
  toggle: () => void
}) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <Button
      type="button"
      size="icon-xs"
      variant="ghost"
      disabled={locked}
      className="absolute end-1 top-1.5 rounded-sm"
      title={t(revealed ? "hideSecret" : "showSecret")}
      aria-label={t(revealed ? "hideSecret" : "showSecret")}
      onClick={toggle}
    >
      {revealed ? <EyeOff /> : <Eye />}
    </Button>
  )
}
