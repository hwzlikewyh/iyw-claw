"use client"

import { useState, type ReactNode } from "react"
import { Boxes, Loader2, Search, Settings2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { Input } from "@/components/ui/input"
import {
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu"
import type { AgentType } from "@/lib/types"
import type { ComposerPluginChoice } from "@/lib/composer-plugins"
import { useComposerPlugins } from "./use-composer-plugins"
import { useComposerPluginSelection } from "./use-composer-plugin-selection"

interface Props {
  tabId: string | null
  agentType: AgentType | null
  workspacePath: string | null
  onSelect: (choice: ComposerPluginChoice, instruction: string) => void
}

export function ConversationPluginMenu(props: Props) {
  const [open, setOpen] = useState(false),
    [query, setQuery] = useState("")
  const state = useComposerPlugins({ ...props, open })
  const selection = useComposerPluginSelection({
    ...props,
    scopeKey: state.scopeKey,
  })
  const normalized = query.trim().toLocaleLowerCase()
  const visible = state.choices.filter((item) =>
    `${item.label} ${item.slug}`.toLocaleLowerCase().includes(normalized)
  )
  return (
    <PluginSubmenu
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) setQuery("")
      }}
      triggerDisabled={!props.agentType || !props.tabId}
    >
      <PluginSearch query={query} onChange={setQuery} />
      <PluginResults
        query={query}
        choices={visible}
        loading={state.loading}
        error={state.error || selection.error}
        busy={selection.busy}
        onRetry={state.refresh}
        onClearSelectionError={selection.clearError}
        onSelect={(choice) => {
          void selection.select(choice).then((accepted) => {
            if (accepted) setOpen(false)
          })
        }}
      />
    </PluginSubmenu>
  )
}

function PluginSubmenu(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
  triggerDisabled: boolean
  children: ReactNode
}) {
  const t = useTranslations("ComposerPlugins")
  return (
    <DropdownMenuSub open={props.open} onOpenChange={props.onOpenChange}>
      <DropdownMenuSubTrigger disabled={props.triggerDisabled}>
        <Boxes className="size-4" />
        {t("menuLabel")}
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent
        className="flex w-72 max-w-[calc(100vw-1rem)] flex-col p-1"
        style={{
          maxHeight:
            "min(32rem, var(--radix-dropdown-menu-content-available-height))",
        }}
        aria-label={t("menuLabel")}
      >
        {props.children}
        <PluginMarketLink />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

function PluginResults(props: {
  query: string
  choices: ComposerPluginChoice[]
  loading: boolean
  error: string
  busy: boolean
  onRetry: () => void
  onClearSelectionError: () => void
  onSelect: (choice: ComposerPluginChoice) => void
}) {
  const t = useTranslations("ComposerPlugins")
  return (
    <>
      <p className="px-3 py-1 text-xs text-muted-foreground">{t("hint")}</p>
      <PluginMenuStatus
        loading={props.loading}
        error={props.error}
        empty={!props.choices.length}
        query={props.query}
        onRetry={() => {
          props.onClearSelectionError()
          props.onRetry()
        }}
      />
      <div className="min-h-0 overflow-y-auto">
        {props.choices.map((choice) => (
          <PluginMenuRow
            key={choice.id}
            choice={choice}
            busy={props.busy}
            onSelect={() => props.onSelect(choice)}
          />
        ))}
      </div>
    </>
  )
}

function PluginSearch({
  query,
  onChange,
}: {
  query: string
  onChange: (query: string) => void
}) {
  const t = useTranslations("ComposerPlugins")
  return (
    <div className="relative m-1 shrink-0">
      <Search className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground" />
      <Input
        value={query}
        onChange={(event) => onChange(event.target.value)}
        className="h-9 pl-9"
        placeholder={t("search")}
        aria-label={t("search")}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown") {
            event.preventDefault()
            event.currentTarget
              .closest('[role="menu"]')
              ?.querySelector<HTMLElement>(
                "[data-plugin-choice]:not([data-disabled])"
              )
              ?.focus()
          }
          if (event.key !== "Escape" && event.key !== "Tab")
            event.stopPropagation()
        }}
      />
    </div>
  )
}

function PluginMenuStatus(props: {
  loading: boolean
  error: string
  empty: boolean
  query: string
  onRetry: () => void
}) {
  const t = useTranslations("ComposerPlugins")
  if (props.loading)
    return (
      <Loader2 className="m-3 size-4 animate-spin" aria-label={t("loading")} />
    )
  if (props.error)
    return (
      <div className="px-3 py-2 text-xs">
        <p role="alert" className="break-words text-destructive">
          {props.error}
        </p>
        <DropdownMenuItem
          onSelect={(event) => {
            event.preventDefault()
            props.onRetry()
          }}
        >
          {t("retry")}
        </DropdownMenuItem>
      </div>
    )
  if (!props.empty) return null
  return (
    <p className="px-3 py-3 text-xs text-muted-foreground">
      {t(props.query.trim() ? "noResults" : "empty")}
    </p>
  )
}

function PluginMenuRow({
  choice,
  busy,
  onSelect,
}: {
  choice: ComposerPluginChoice
  busy: boolean
  onSelect: () => void
}) {
  const t = useTranslations("ComposerPlugins")
  return (
    <DropdownMenuItem
      data-plugin-choice
      disabled={busy}
      textValue={choice.label}
      className="min-h-11"
      onSelect={(event) => {
        event.preventDefault()
        onSelect()
      }}
    >
      <Boxes className="size-4 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">{choice.label}</span>
        <span className="block truncate text-xs text-muted-foreground">
          {choice.blocked
            ? t("manageRequired")
            : choice.skills.length
              ? t("skills", { count: choice.skills.length })
              : t("tools")}
        </span>
      </span>
      {choice.blocked ? (
        <Settings2 className="size-3.5 text-muted-foreground" />
      ) : null}
    </DropdownMenuItem>
  )
}

function PluginMarketLink() {
  const t = useTranslations("ComposerPlugins"),
    route = useWorkbenchRoute()
  return (
    <>
      <DropdownMenuSeparator />
      <DropdownMenuItem
        className="shrink-0"
        onSelect={() => route.openPluginMarket()}
      >
        <Settings2 className="size-4" />
        {t("manage")}
      </DropdownMenuItem>
    </>
  )
}
