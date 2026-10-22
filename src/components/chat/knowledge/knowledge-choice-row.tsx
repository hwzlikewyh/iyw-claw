"use client"

import { useId, type ReactNode } from "react"
import { FileText, Folder } from "lucide-react"
import { Checkbox } from "@/components/ui/checkbox"
import { cn } from "@/lib/utils"
import type { KnowledgeChoice } from "@/lib/cloud-knowledge"

interface ChoiceRowProps {
  choice: KnowledgeChoice
  checked: boolean | "indeterminate"
  disabled: boolean
  onToggle: (choice: KnowledgeChoice) => void
  children: ReactNode
  status: ReactNode
}

export function KnowledgeChoiceRow(props: ChoiceRowProps) {
  const { choice, checked, disabled, onToggle, children, status } = props
  const controlId = useId()
  const Icon = choice.kind === "folder" ? Folder : FileText
  return (
    <div
      className={cn(
        "flex min-h-10 items-center gap-2 rounded-sm pr-3 text-sm hover:bg-muted/50",
        checked === true && "bg-muted/40"
      )}
    >
      {children}
      <Checkbox
        id={controlId}
        checked={checked}
        disabled={disabled}
        onCheckedChange={() => onToggle(choice)}
      />
      <Icon className="size-4 shrink-0 text-muted-foreground" />
      <label
        htmlFor={controlId}
        title={choice.name}
        className="min-w-0 flex-1 cursor-pointer truncate py-2"
      >
        {choice.name}
      </label>
      <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
        {status}
      </span>
    </div>
  )
}
