"use client"

import { FileText, Folder, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { knowledgeChoiceKey, type KnowledgeChoice } from "@/lib/cloud-knowledge"

interface SelectedSourcesProps {
  selected: KnowledgeChoice[]
  onToggle: (choice: KnowledgeChoice) => void
  disabled: boolean
}

export function SelectedSources(props: SelectedSourcesProps) {
  const { selected } = props
  const t = useTranslations("CloudKnowledge")
  return (
    <aside
      className="flex max-h-32 shrink-0 flex-col rounded-md bg-muted/30 p-3 sm:max-h-none sm:w-60"
      aria-label={t("selected", { count: selected.length })}
    >
      <p className="mb-2 text-sm font-medium">
        {t("selected", { count: selected.length })}
      </p>
      {!selected.length && (
        <p className="text-xs leading-relaxed text-muted-foreground">
          {t("unselected")}
        </p>
      )}
      <div className="min-h-0 overflow-y-auto">
        {selected.map((item) => (
          <SelectedSource
            key={knowledgeChoiceKey(item)}
            choice={item}
            onToggle={props.onToggle}
            disabled={props.disabled}
          />
        ))}
      </div>
    </aside>
  )
}

function SelectedSource({
  choice,
  onToggle,
  disabled,
}: Omit<SelectedSourcesProps, "selected"> & { choice: KnowledgeChoice }) {
  const t = useTranslations("CloudKnowledge")
  const Icon = choice.kind === "folder" ? Folder : FileText
  return (
    <div className="flex min-h-10 items-center gap-2">
      <Icon className="size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm" title={choice.name}>
          {choice.name}
        </p>
        <p className="text-xs text-muted-foreground">
          {t(choice.category === 0 ? "knowledge" : "customers")}
          {choice.kind === "folder" && ` · ${t("includesChildren")}`}
        </p>
      </div>
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={disabled}
        aria-label={t("remove", { name: choice.name })}
        onClick={() => onToggle(choice)}
      >
        <X className="size-3.5" />
      </Button>
    </div>
  )
}
