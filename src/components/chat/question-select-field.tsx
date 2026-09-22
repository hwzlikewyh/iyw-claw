import { useState } from "react"
import { useTranslations } from "next-intl"
import { Check, ChevronDown } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { splitRecommended } from "@/lib/ask-question"
import { questionControl } from "@/lib/question-answer"
import type { QuestionFieldProps } from "./question-field-props"

export function QuestionSelectField(props: QuestionFieldProps) {
  return questionControl(props.question) === "combobox" ? (
    <SearchQuestionSelect {...props} />
  ) : (
    <SimpleQuestionSelect {...props} />
  )
}

function SimpleQuestionSelect({
  question,
  value,
  locked,
  onSelect,
}: QuestionFieldProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  const selected = question.options.findIndex((option) =>
    value.chosen.includes(option.label)
  )
  return (
    <Select
      value={selected < 0 ? "" : String(selected)}
      disabled={locked}
      onValueChange={(index) => onSelect(question.options[Number(index)].label)}
    >
      <SelectTrigger
        aria-label={question.header}
        className="w-full min-w-0 rounded-md text-xs"
      >
        <SelectValue placeholder={t("selectPlaceholder")} />
      </SelectTrigger>
      <SelectContent
        className="max-w-[min(90vw,30rem)] rounded-md"
        position="popper"
      >
        {question.options.map((option, index) => (
          <SelectItem
            className="rounded-sm text-xs"
            key={option.label}
            value={String(index)}
          >
            {splitRecommended(option.label).text}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function SearchQuestionSelect({
  question,
  value,
  locked,
  onSelect,
}: QuestionFieldProps) {
  const t = useTranslations("Folder.chat.askQuestion")
  const [open, setOpen] = useState(false)
  return (
    <Popover open={open && !locked} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-label={question.header}
          aria-expanded={open}
          disabled={locked}
          className="w-full min-w-0 justify-between rounded-md text-xs"
        >
          <span className="truncate">
            {value.chosen[0]
              ? splitRecommended(value.chosen[0]).text
              : t("selectPlaceholder")}
          </span>
          <ChevronDown className="size-3.5" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-[var(--radix-popover-trigger-width)] max-w-[90vw] rounded-md p-0"
      >
        <SearchOptions
          question={question}
          value={value}
          onSelect={(label) => {
            if (!value.chosen.includes(label)) onSelect(label)
            setOpen(false)
          }}
        />
      </PopoverContent>
    </Popover>
  )
}

function SearchOptions({
  question,
  value,
  onSelect,
}: Pick<QuestionFieldProps, "question" | "value" | "onSelect">) {
  const t = useTranslations("Folder.chat.askQuestion")
  return (
    <Command>
      <CommandInput
        aria-label={t("searchOptions")}
        placeholder={t("searchOptions")}
      />
      <CommandList className="max-h-52">
        <CommandEmpty>{t("noOptions")}</CommandEmpty>
        {question.options.map((option) => (
          <CommandItem
            key={option.label}
            value={option.label}
            onSelect={() => onSelect(option.label)}
          >
            <span className="min-w-0 flex-1 break-words text-xs">
              {splitRecommended(option.label).text}
            </span>
            {value.chosen.includes(option.label) && (
              <Check className="size-3.5" />
            )}
          </CommandItem>
        ))}
      </CommandList>
    </Command>
  )
}
