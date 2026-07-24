"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Brain, Loader2, PencilLine } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { MessageAction } from "@/components/ai-elements/message"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import {
  appendUserMemoryDirect,
  correctUserMemory,
  getUserMemorySettings,
} from "@/lib/api"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import type { AgentType } from "@/lib/types"
import {
  USER_MEMORY_DOCUMENTS,
  type UserMemoryDocumentId,
  type UserMemorySettingsSnapshot,
} from "@/lib/user-memory-documents"

interface UserMemoryMessageActionsProps {
  content: string
  agentType: AgentType
}

const actionClassName =
  "opacity-0 transition-opacity group-hover/user-msg:opacity-100 self-end"

function writableDocument(
  settings: UserMemorySettingsSnapshot,
  document: UserMemoryDocumentId
): boolean {
  const snapshot = settings.documents[document]
  return snapshot.readable && !snapshot.readonly
}

function initialDocument(
  settings: UserMemorySettingsSnapshot
): UserMemoryDocumentId {
  return (
    USER_MEMORY_DOCUMENTS.find((item) => writableDocument(settings, item.id))
      ?.id ?? "memory"
  )
}

function RememberButton({ content, agentType }: UserMemoryMessageActionsProps) {
  const t = useTranslations("Folder.chat.messageList.userMemory")
  const [saving, setSaving] = useState(false)

  const remember = useCallback(async () => {
    if (saving || !content.trim()) return
    setSaving(true)
    try {
      const result = await appendUserMemoryDirect({ content, agentType })
      toast.success(t(result.appended ? "remembered" : "alreadyRemembered"), {
        description: t("newConversationRequired"),
      })
    } catch (error) {
      toast.error(t("rememberFailed"), { description: toErrorMessage(error) })
    } finally {
      setSaving(false)
    }
  }, [agentType, content, saving, t])

  return (
    <MessageAction
      tooltip={t("remember")}
      className={actionClassName}
      disabled={saving || !content.trim()}
      onClick={() => void remember()}
      size="icon-xs"
    >
      {saving ? (
        <Loader2 className="animate-spin" size={12} />
      ) : (
        <Brain size={12} />
      )}
    </MessageAction>
  )
}

interface CorrectionDialogProps {
  content: string
  open: boolean
  onOpenChange: (open: boolean) => void
}

function CorrectionDialog({
  content,
  open,
  onOpenChange,
}: CorrectionDialogProps) {
  const t = useTranslations("Folder.chat.messageList.userMemory")
  const [settings, setSettings] = useState<UserMemorySettingsSnapshot | null>(
    null
  )
  const [document, setDocument] = useState<UserMemoryDocumentId>("memory")
  const [oldContent, setOldContent] = useState("")
  const [newContent, setNewContent] = useState(content)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    let active = true
    setLoading(true)
    setError(null)
    setOldContent("")
    setNewContent(content)
    void getUserMemorySettings()
      .then((next) => {
        if (!active) return
        setSettings(next)
        setDocument(initialDocument(next))
      })
      .catch((loadError) => active && setError(toErrorMessage(loadError)))
      .finally(() => active && setLoading(false))
    return () => {
      active = false
    }
  }, [content, open])

  const selectedSnapshot = settings?.documents[document] ?? null
  const canSubmit = useMemo(
    () =>
      !loading &&
      !saving &&
      !!selectedSnapshot?.readable &&
      !selectedSnapshot.readonly &&
      oldContent.trim().length > 0 &&
      newContent.trim().length > 0 &&
      oldContent.trim() !== newContent.trim(),
    [loading, newContent, oldContent, saving, selectedSnapshot]
  )

  const submit = useCallback(async () => {
    if (!canSubmit || !selectedSnapshot) return
    setSaving(true)
    setError(null)
    try {
      await correctUserMemory({
        document,
        oldContent,
        newContent,
        expectedEtag: selectedSnapshot.etag,
      })
      toast.success(t("corrected"), {
        description: t("newConversationRequired"),
      })
      onOpenChange(false)
    } catch (submitError) {
      const conflict = extractAppCommandError(submitError)?.code === "conflict"
      setError(conflict ? t("conflict") : toErrorMessage(submitError))
    } finally {
      setSaving(false)
    }
  }, [
    canSubmit,
    document,
    newContent,
    oldContent,
    onOpenChange,
    selectedSnapshot,
    t,
  ])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg rounded-lg">
        <DialogHeader>
          <DialogTitle>{t("correctTitle")}</DialogTitle>
          <DialogDescription>{t("correctDescription")}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="memory-document">{t("document")}</Label>
            <Select
              value={document}
              disabled={loading || saving || !settings}
              onValueChange={(value) =>
                setDocument(value as UserMemoryDocumentId)
              }
            >
              <SelectTrigger id="memory-document" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {USER_MEMORY_DOCUMENTS.map((item) => (
                  <SelectItem
                    key={item.id}
                    value={item.id}
                    disabled={
                      settings ? !writableDocument(settings, item.id) : true
                    }
                  >
                    {t(`documents.${item.id}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="grid gap-2">
            <Label htmlFor="old-memory-content">{t("oldContent")}</Label>
            <Textarea
              id="old-memory-content"
              value={oldContent}
              disabled={loading || saving}
              placeholder={t("oldContentPlaceholder")}
              onChange={(event) => setOldContent(event.target.value)}
              className="min-h-24 resize-y"
            />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="new-memory-content">{t("newContent")}</Label>
            <Textarea
              id="new-memory-content"
              value={newContent}
              disabled={loading || saving}
              onChange={(event) => setNewContent(event.target.value)}
              className="min-h-24 resize-y"
            />
          </div>
          {error && (
            <div
              role="alert"
              className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400"
            >
              {error}
            </div>
          )}
        </div>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={saving}
            onClick={() => onOpenChange(false)}
          >
            {t("cancel")}
          </Button>
          <Button
            type="button"
            disabled={!canSubmit}
            onClick={() => void submit()}
          >
            {saving && <Loader2 className="animate-spin" />}
            {saving ? t("saving") : t("saveCorrection")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function UserMemoryMessageActions({
  content,
  agentType,
}: UserMemoryMessageActionsProps) {
  const t = useTranslations("Folder.chat.messageList.userMemory")
  const [correctionOpen, setCorrectionOpen] = useState(false)

  return (
    <>
      <RememberButton content={content} agentType={agentType} />
      <MessageAction
        tooltip={t("correct")}
        className={actionClassName}
        disabled={!content.trim()}
        onClick={() => setCorrectionOpen(true)}
        size="icon-xs"
      >
        <PencilLine size={12} />
      </MessageAction>
      <CorrectionDialog
        content={content}
        open={correctionOpen}
        onOpenChange={setCorrectionOpen}
      />
    </>
  )
}
