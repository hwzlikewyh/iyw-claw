"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useTabStore } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import {
  getConversationConnectors,
  prepareConnectorConversation,
  setConversationConnectors,
} from "@/lib/conversation-connectors"
import { mcpRemoveServer } from "@/lib/api"
import type { LocalMcpServer } from "@/lib/types"
import { ConnectorOAuthDialog } from "./connector-oauth-dialog"

export function ConnectorInstalledList({
  items,
  onChanged,
}: {
  items: LocalMcpServer[]
  onChanged: () => Promise<void>
}) {
  const t = useTranslations("ConnectorMarket")
  const [removing, setRemoving] = useState<string | null>(null)
  const [oauth, setOAuth] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const activeTabId = useTabStore((state) => state.activeTabId)
  const route = useWorkbenchRoute()
  const enableConnector = async (id: string) => {
    const tab = useTabStore
      .getState()
      .tabs.find((value) => value.id === activeTabId)
    if (!tab || busy) return
    setBusy(true)
    try {
      const current = await getConversationConnectors(
        tab.conversationId ?? null,
        tab.agentType
      )
      const choice = current.connectors.find((value) => value.id === id)
      if (!choice?.compatible)
        throw new Error(choice?.reason ?? t("incompatible"))
      const prepared = await prepareConnectorConversation(
        tab.id,
        t("newConversation")
      )
      await setConversationConnectors({
        conversationId: prepared.id,
        revision: current.revision,
        selected: [...new Set([...current.selected, id])],
      })
      prepared.bind()
      route.openConversations()
      toast.success(t("conversationEnabled"))
    } catch (cause) {
      toast.error(String(cause))
    } finally {
      setBusy(false)
    }
  }
  const uninstall = async () => {
    if (!removing) return
    setBusy(true)
    try {
      await mcpRemoveServer(removing)
      setRemoving(null)
      await onChanged()
    } catch (cause) {
      toast.error(String(cause))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="space-y-2 pt-3">
      {!items.length && (
        <p className="py-8 text-center text-sm text-muted-foreground">
          {t("installedEmpty")}
        </p>
      )}
      {items.map((item) => (
        <div
          key={item.id}
          className="flex flex-wrap items-center gap-3 rounded-lg border p-4"
        >
          <div className="min-w-0 flex-1">
            <strong className="text-sm">{item.display_name || item.id}</strong>
            <p className="text-xs text-muted-foreground">
              {t("chooseInConversation")}
            </p>
          </div>
          <Badge variant="outline">{t("configured")}</Badge>
          {(item.spec.iyw_market as { auth_mode?: string } | undefined)
            ?.auth_mode === "oauth" && (
            <Button
              size="sm"
              variant="outline"
              disabled={busy}
              onClick={() => setOAuth(item.id)}
            >
              {t("authorize")}
            </Button>
          )}
          <Button
            size="sm"
            disabled={busy || !activeTabId}
            title={!activeTabId ? t("openConversationFirst") : undefined}
            onClick={() => void enableConnector(item.id)}
          >
            {t("useInConversation")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => setRemoving(item.id)}
          >
            {t("uninstall")}
          </Button>
        </div>
      ))}
      {oauth && (
        <ConnectorOAuthDialog id={oauth} onClose={() => setOAuth(null)} />
      )}
      <Dialog
        open={removing !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setRemoving(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("uninstall")}</DialogTitle>
            <DialogDescription>{t("uninstallConfirm")}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => setRemoving(null)}
            >
              {t("cancel")}
            </Button>
            <Button
              variant="destructive"
              disabled={busy}
              onClick={() => void uninstall()}
            >
              {t("uninstall")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
