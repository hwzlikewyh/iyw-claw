"use client"

import { useState } from "react"
import { BookOpenText, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useOptionalIywAccount } from "@/contexts/iyw-account-context"
import { useTabStore } from "@/contexts/tab-context"
import { useAccountRequestsBlocked } from "@/components/account/use-account-request-guard"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import type { KnowledgeCategory, KnowledgeChoice } from "@/lib/cloud-knowledge"
import {
  KnowledgeLoadError,
  KnowledgeLoading,
  KnowledgeTree,
} from "./knowledge-tree"
import { SelectedSources } from "./selected-sources"
import { useKnowledgeSelection } from "./use-knowledge-selection"

interface KnowledgeDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  tabId?: string | null
}

export function KnowledgeDialog({
  open,
  onOpenChange,
  tabId,
}: KnowledgeDialogProps) {
  const t = useTranslations("CloudKnowledge")
  const account = useOptionalIywAccount()
  const blocked = useAccountRequestsBlocked()
  const conversationId = useTabStore(
    (state) =>
      state.tabs.find((tab) => tab.id === tabId)?.conversationId ?? null
  )
  const accountKey = [
    account?.status,
    account?.profile?.user_id,
    account?.profile?.org_code,
    blocked,
  ].join(":")
  const onSaved = () => {
    onOpenChange(false)
    toast.success(t("saved"))
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[min(42rem,calc(100dvh-2rem))] max-w-[min(54rem,calc(100vw-2rem))] flex-col gap-3 overflow-hidden rounded-lg p-4 sm:max-w-[min(54rem,calc(100vw-2rem))]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <BookOpenText className="size-4" />
            {t("title")}
          </DialogTitle>
          <DialogDescription>{t("description")}</DialogDescription>
        </DialogHeader>
        {open && tabId && !blocked && (
          <KnowledgePicker
            key={tabId + accountKey + conversationId}
            tabId={tabId}
            onSaved={onSaved}
          />
        )}
        {blocked && (
          <p role="status" className="p-4 text-sm text-muted-foreground">
            {t("accountBusy")}
          </p>
        )}
      </DialogContent>
    </Dialog>
  )
}

function KnowledgePicker({
  tabId,
  onSaved,
}: {
  tabId: string
  onSaved: () => void
}) {
  const picker = useKnowledgeSelection(tabId, onSaved)
  if (!picker.data)
    return picker.error ? (
      <KnowledgeLoadError error={picker.error} retry={picker.retry} />
    ) : (
      <KnowledgeLoading />
    )
  return (
    <>
      <KnowledgeBrowser
        selected={picker.data.selected}
        onToggle={picker.toggle}
        disabled={picker.busy}
      />
      {picker.error && (
        <KnowledgeLoadError error={picker.error} retry={picker.retry} />
      )}
      <PickerFooter picker={picker} />
    </>
  )
}

interface BrowseProps {
  selected: KnowledgeChoice[]
  onToggle: (choice: KnowledgeChoice) => void
  disabled: boolean
}

function KnowledgeBrowser(props: BrowseProps) {
  const [category, setCategory] = useState<KnowledgeCategory>(0)
  const [query, setQuery] = useState("")
  const changeCategory = (value: KnowledgeCategory) => {
    setCategory(value)
    setQuery("")
  }
  return (
    <>
      <BrowseToolbar
        category={category}
        onCategoryChange={changeCategory}
        query={query}
        onQueryChange={setQuery}
      />
      <div className="flex min-h-0 flex-1 flex-col gap-3 sm:flex-row">
        <div className="min-h-0 min-w-0 flex-1 overflow-auto rounded-md border">
          <KnowledgeTree
            key={category}
            {...props}
            category={category}
            query={query.trim().toLocaleLowerCase()}
          />
        </div>
        <SelectedSources {...props} />
      </div>
    </>
  )
}

function BrowseToolbar({
  category,
  onCategoryChange,
  query,
  onQueryChange,
}: {
  category: KnowledgeCategory
  onCategoryChange: (value: KnowledgeCategory) => void
  query: string
  onQueryChange: (query: string) => void
}) {
  const t = useTranslations("CloudKnowledge")
  return (
    <>
      <div
        role="group"
        aria-label={t("title")}
        className="flex shrink-0 gap-1 rounded-md bg-muted p-1"
      >
        {([0, 1] as const).map((value) => (
          <Button
            key={value}
            variant={category === value ? "secondary" : "ghost"}
            size="sm"
            className="flex-1 rounded-sm"
            aria-pressed={category === value}
            onClick={() => onCategoryChange(value)}
          >
            {t(value === 0 ? "knowledge" : "customers")}
          </Button>
        ))}
      </div>
      <Input
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder={t("filterFolders")}
        aria-label={t("filterFolders")}
        className="shrink-0"
      />
    </>
  )
}

function PickerFooter({
  picker,
}: {
  picker: ReturnType<typeof useKnowledgeSelection>
}) {
  const t = useTranslations("CloudKnowledge")
  return (
    <div className="flex shrink-0 items-center gap-3">
      <p className="min-w-0 flex-1 text-xs leading-relaxed text-muted-foreground">
        {t("scopeHint")}
      </p>
      <Button
        variant="ghost"
        size="sm"
        disabled={picker.busy || !picker.data?.selected.length}
        onClick={picker.clear}
      >
        {t("clear")}
      </Button>
      <Button
        size="sm"
        disabled={picker.busy}
        onClick={() => void picker.save()}
      >
        {picker.busy && <Loader2 className="size-4 animate-spin" />}
        {t("confirm")}
      </Button>
    </div>
  )
}
