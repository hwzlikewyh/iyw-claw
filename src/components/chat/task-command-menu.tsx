"use client"

import { ListRestart, Target, Workflow } from "lucide-react"
import { useTranslations } from "next-intl"

import { DropdownRadioItemContent } from "@/components/chat/dropdown-radio-item-content"
import {
  commandToReference,
  skillToReference,
} from "@/components/chat/composer/invocation-reference"
import {
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu"
import type { ReferenceAttrs } from "@/components/chat/composer/types"
import type { AgentSkillItem, AvailableCommandInfo } from "@/lib/types"

interface TaskCommandMenuProps {
  commands: AvailableCommandInfo[]
  skills: AgentSkillItem[]
  skillPrefix: "/" | "$"
  onSelect: (reference: ReferenceAttrs) => void
}

const TASK_COMMANDS = [
  { name: "goal", labelKey: "goal", icon: Target },
  { name: "loop", labelKey: "loop", icon: ListRestart },
] as const

function normalizeCommandName(name: string): string {
  return name.trim().replace(/^\/+/, "").toLowerCase()
}

function resolveTaskCommands(
  commands: AvailableCommandInfo[],
  skills: AgentSkillItem[],
  skillPrefix: "/" | "$"
) {
  return TASK_COMMANDS.flatMap((definition) => {
    const command = commands.find(
      (item) => normalizeCommandName(item.name) === definition.name
    )
    const skill = skills.find(
      (item) => normalizeCommandName(item.id) === definition.name
    )
    if (!command && !skill) return []
    const reference = command
      ? commandToReference({
          ...command,
          name: normalizeCommandName(command.name),
        })
      : skillToReference(skill!, skillPrefix)
    return [{ ...definition, reference }]
  })
}

export function TaskCommandMenu({
  commands,
  skills,
  skillPrefix,
  onSelect,
}: TaskCommandMenuProps) {
  const t = useTranslations("Folder.chat.messageInput.taskCommands")
  const available = resolveTaskCommands(commands, skills, skillPrefix)

  if (available.length === 0) return null

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <Workflow className="size-4" />
        {t("title")}
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="min-w-72">
        {available.map(({ name, labelKey, icon: Icon, reference }) => (
          <DropdownMenuItem key={name} onClick={() => onSelect(reference)}>
            <Icon className="size-4" />
            <DropdownRadioItemContent
              label={t(`${labelKey}.label`)}
              description={t(`${labelKey}.description`)}
            />
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}
