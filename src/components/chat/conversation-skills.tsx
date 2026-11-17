"use client"

import { useRef, useState, type RefObject } from "react"
import { BookOpenText, Search } from "lucide-react"
import { useTranslations } from "next-intl"
import { Input } from "@/components/ui/input"
import {
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu"
import type { AgentSkillItem } from "@/lib/types"

interface Props {
  skills: AgentSkillItem[]
  disabled: boolean
  scanning: boolean
  scanFailed: boolean
  onOpenChange: (open: boolean) => void
  onSelect: (skill: AgentSkillItem) => void
}

export function ConversationSkillMenu(props: Props) {
  const t = useTranslations("Folder.chat.messageInput")
  const [query, setQuery] = useState("")
  const inputRef = useRef<HTMLInputElement>(null)
  return (
    <DropdownMenuSub
      onOpenChange={(open) => {
        setQuery("")
        props.onOpenChange(open)
      }}
    >
      <DropdownMenuSubTrigger disabled={props.disabled}>
        <BookOpenText className="size-4" />
        {t("skills")}
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent
        className="flex w-80 max-w-[calc(100vw-1rem)] flex-col"
        style={{
          maxHeight:
            "min(32rem, var(--radix-dropdown-menu-content-available-height))",
        }}
        aria-label={t("skills")}
        onFocus={(event) => {
          if (event.target !== event.currentTarget) return
          event.preventDefault()
          inputRef.current?.focus()
        }}
      >
        <SkillSearch inputRef={inputRef} query={query} onChange={setQuery} />
        <div className="min-h-0 overflow-y-auto">
          <SkillMenuItems {...props} query={query} />
        </div>
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

function SkillSearch({
  inputRef,
  query,
  onChange,
}: {
  inputRef: RefObject<HTMLInputElement | null>
  query: string
  onChange: (query: string) => void
}) {
  const t = useTranslations("Folder.chat.messageInput")
  return (
    <div className="relative m-1 shrink-0">
      <Search className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground" />
      <Input
        ref={inputRef}
        value={query}
        onChange={(event) => onChange(event.target.value)}
        className="h-9 pl-9"
        placeholder={t("skillsSearch")}
        aria-label={t("skillsSearch")}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) {
            event.stopPropagation()
            return
          }
          if (event.key === "ArrowDown") {
            event.preventDefault()
            event.currentTarget
              .closest('[role="menu"]')
              ?.querySelector<HTMLElement>("[data-skill-choice]")
              ?.focus()
          }
          if (event.key !== "Escape" && event.key !== "Tab")
            event.stopPropagation()
        }}
      />
    </div>
  )
}

function SkillMenuItems(props: Props & { query: string }) {
  const query = props.query.trim().toLocaleLowerCase()
  const visible = props.skills.filter((skill) =>
    `${skill.name} ${skill.id} ${skill.description ?? ""}`
      .toLocaleLowerCase()
      .includes(query)
  )
  if (!visible.length) return <SkillMenuStatus {...props} />
  return visible.map((skill) => (
    <DropdownMenuItem
      key={`${skill.scope}-${skill.id}`}
      data-skill-choice
      textValue={skill.name || skill.id}
      onSelect={() => props.onSelect(skill)}
    >
      <BookOpenText className="size-4" />
      <span className="min-w-0 flex-1">
        <span className="block truncate">{skill.name || skill.id}</span>
        {skill.description && (
          <span className="block truncate text-xs text-muted-foreground">
            {skill.description}
          </span>
        )}
      </span>
    </DropdownMenuItem>
  ))
}

function SkillMenuStatus(props: Props) {
  const t = useTranslations("Folder.chat.messageInput")
  let message:
    | "skillsLoading"
    | "skillsScanFailed"
    | "skillsEmpty"
    | "skillsNoResults" = "skillsNoResults"
  if (!props.skills.length) {
    message = props.scanning
      ? "skillsLoading"
      : props.scanFailed
        ? "skillsScanFailed"
        : "skillsEmpty"
  }
  return (
    <div
      role="status"
      className={`px-3 py-4 text-center text-xs ${message === "skillsScanFailed" ? "text-destructive" : "text-muted-foreground"}`}
    >
      {t(message)}
    </div>
  )
}
