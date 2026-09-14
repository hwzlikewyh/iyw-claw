"use client"

import { useState } from "react"
import {
  Check,
  ChevronsUpDown,
  LayoutGrid,
  List,
  RefreshCw,
  Search,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"

export interface ResourceSessionOption {
  id: number
  title: string
}

interface ResourceToolbarProps {
  search: string
  onSearchChange: (value: string) => void
  session: string
  sessionOptions: ResourceSessionOption[]
  onSessionChange: (value: string) => void
  view: "grid" | "list"
  onViewChange: (value: "grid" | "list") => void
  busy: boolean
  onRefresh: () => void
}

export function ResourceToolbar(props: ResourceToolbarProps) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b pb-4 sm:gap-3">
      <ResourceSearch {...props} />
      <ResourceSessionFilter {...props} />
      <div className="flex shrink-0 items-center gap-2 sm:ms-auto">
        <ResourceViewSwitch {...props} />
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={props.busy}
          onClick={props.onRefresh}
          aria-label={t("refresh")}
          title={t("refresh")}
        >
          <RefreshCw className={cn("size-4", props.busy && "animate-spin")} />
        </Button>
      </div>
    </div>
  )
}

function ResourceSessionFilter({
  session,
  sessionOptions,
  onSessionChange,
}: ResourceToolbarProps) {
  const t = useTranslations("Resources")
  const artifactT = useTranslations("Folder.taskArtifacts")
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const selectedOption = sessionOptions.find(
    (option) => String(option.id) === session
  )
  const selectedLabel =
    session === "all"
      ? t("allConversations")
      : selectedOption?.title || artifactT("untitled")

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen)
    if (!nextOpen) setQuery("")
  }

  const handleSelect = (value: string) => {
    onSessionChange(value)
    setOpen(false)
    setQuery("")
  }

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="default"
          className="w-full min-w-0 max-w-full justify-between rounded-md bg-muted/25 sm:w-56"
          aria-label={t("sessionFilterLabel")}
          aria-expanded={open}
        >
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate">{selectedLabel}</span>
          </span>
          <ChevronsUpDown className="size-4 shrink-0 text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-[min(22rem,calc(100vw-2rem))] p-0"
      >
        <Command shouldFilter>
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder={t("sessionFilterSearchPlaceholder")}
            aria-label={t("sessionFilterSearchLabel")}
          />
          <CommandList className="max-h-72">
            <CommandEmpty>{t("sessionFilterEmpty")}</CommandEmpty>
            <CommandGroup>
              <CommandItem
                value={`all ${t("allConversations")}`}
                onSelect={() => handleSelect("all")}
              >
                <span className="truncate">{t("allConversations")}</span>
                <Check
                  className={cn(
                    "ms-auto size-4",
                    session === "all" ? "opacity-100" : "opacity-0"
                  )}
                />
              </CommandItem>
              {sessionOptions.map((option) => {
                const value = String(option.id)
                const label = option.title || artifactT("untitled")
                return (
                  <CommandItem
                    key={option.id}
                    value={`${value} ${label}`}
                    onSelect={() => handleSelect(value)}
                  >
                    <span className="truncate">{label}</span>
                    <Check
                      className={cn(
                        "ms-auto size-4",
                        session === value ? "opacity-100" : "opacity-0"
                      )}
                    />
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

function ResourceSearch({ search, onSearchChange }: ResourceToolbarProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const r = useTranslations("Resources")
  return (
    <div className="relative min-w-0 basis-full sm:flex-1 sm:basis-64">
      <Search className="pointer-events-none absolute start-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input
        value={search}
        onChange={(event) => onSearchChange(event.target.value)}
        placeholder={t("searchPlaceholder")}
        aria-label={t("searchLabel")}
        className="h-9 rounded-md bg-muted/25 ps-9 pe-9 text-sm"
      />
      {search && (
        <Button
          variant="ghost"
          size="icon-xs"
          className="absolute end-1.5 top-1/2 -translate-y-1/2"
          onClick={() => onSearchChange("")}
          aria-label={r("clearSearch")}
          title={r("clearSearch")}
        >
          <X className="size-3.5" />
        </Button>
      )}
    </div>
  )
}

function ResourceViewSwitch({ view, onViewChange }: ResourceToolbarProps) {
  const t = useTranslations("Resources")
  return (
    <div
      className="flex items-center gap-0.5 rounded-md bg-muted p-0.5"
      role="group"
      aria-label={t("viewLabel")}
    >
      {(
        [
          ["grid", LayoutGrid],
          ["list", List],
        ] as const
      ).map(([value, Icon]) => (
        <Button
          key={value}
          variant="ghost"
          size="icon-sm"
          className={cn(
            "rounded-sm text-muted-foreground",
            view === value && "bg-background text-foreground shadow-xs"
          )}
          aria-pressed={view === value}
          aria-label={t(value)}
          title={t(value)}
          onClick={() => onViewChange(value)}
        >
          <Icon className="size-4" />
        </Button>
      ))}
    </div>
  )
}
