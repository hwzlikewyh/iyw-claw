"use client"

import { ChevronDown, Focus, RefreshCw, Workflow, X } from "lucide-react"
import { useTranslations } from "next-intl"

import { DropdownRadioItemContent } from "@/components/chat/dropdown-radio-item-content"
import {
  commandToReference,
  skillToReference,
} from "@/components/chat/composer/invocation-reference"
import {
  DropdownMenuItem,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
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
  { name: "goal", labelKey: "goal", icon: Focus },
  { name: "loop", labelKey: "loop", icon: RefreshCw },
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

interface TaskModeRailProps extends Omit<TaskCommandMenuProps, "onSelect"> {
  task: ReferenceAttrs
  running: boolean
  onSelect: (reference: ReferenceAttrs) => void
  onRemove: () => void
}

export function TaskModeRail({
  task,
  running,
  commands,
  skills,
  skillPrefix,
  onSelect,
  onRemove,
}: TaskModeRailProps) {
  const t = useTranslations("Folder.chat.messageInput.taskCommands")
  const available = resolveTaskCommands(commands, skills, skillPrefix)
  const taskName = normalizeCommandName(task.id)
  const current =
    available.find((item) => item.name === taskName) ??
    TASK_COMMANDS.find((item) => item.name === taskName) ??
    TASK_COMMANDS[0]
  const Icon = current.icon

  return (
    <div className="iyw-claw-task-rail mx-2 mt-2 flex h-8 items-center gap-1 rounded-[4px] border border-sky-500/25 bg-sky-500/[0.07] px-1.5 text-sky-800 dark:text-sky-300">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-2 rounded-[3px] px-1.5 py-1 text-left hover:bg-sky-500/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            aria-label={t("replace", {
              task: t(`${current.labelKey}.label`),
            })}
          >
            <Icon className="size-3.5 shrink-0" aria-hidden="true" />
            <span className="truncate text-xs font-medium">
              {t(`${current.labelKey}.label`)}
            </span>
            <ChevronDown
              className="size-3 shrink-0 text-sky-700/70 dark:text-sky-300/70"
              aria-hidden="true"
            />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="min-w-72">
          {available.map(({ name, labelKey, icon: OptionIcon, reference }) => (
            <DropdownMenuItem key={name} onClick={() => onSelect(reference)}>
              <OptionIcon className="size-4" />
              <DropdownRadioItemContent
                label={t(`${labelKey}.label`)}
                description={t(`${labelKey}.description`)}
              />
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <span
        className="flex shrink-0 items-center gap-1.5 px-1 text-[11px] text-sky-700 dark:text-sky-300"
        role="status"
      >
        <span
          className={
            running
              ? "size-1.5 rounded-full bg-sky-500 animate-pulse motion-reduce:animate-none"
              : "size-1.5 rounded-full bg-sky-500/70"
          }
          aria-hidden="true"
        />
        {t(running ? "running" : "enabled")}
      </span>
      <button
        type="button"
        onClick={onRemove}
        className="inline-flex size-6 shrink-0 items-center justify-center rounded-[3px] text-sky-700/70 hover:bg-sky-500/10 hover:text-sky-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring dark:text-sky-300/70 dark:hover:text-sky-100"
        aria-label={t("remove", { task: t(`${current.labelKey}.label`) })}
        title={t("remove", { task: t(`${current.labelKey}.label`) })}
      >
        <X className="size-3.5" aria-hidden="true" />
      </button>
    </div>
  )
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
