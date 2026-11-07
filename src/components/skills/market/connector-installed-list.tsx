"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
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
import { ConnectorInstalledStatus } from "./connector-installed-status"
import { ConnectorMigrationDialog } from "@/components/connectors/connector-migration-dialog"

export function ConnectorInstalledList({
  items,
  onChanged,
}: {
  items: LocalMcpServer[]
  onChanged: () => Promise<void>
}) {
  const t = useTranslations("ConnectorMarket")
  const [removing, setRemoving] = useState<string | null>(null)
  const [migration, setMigration] = useState<{
    targetId: string
  } | null>(null)
  const [busy, setBusy] = useState(false)
  const activeTabId = useTabStore((state) => state.activeTabId)
  const route = useWorkbenchRoute()
  const enableConnector = async (id: string, migrationConfirmed = false) => {
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
        throw new Error(choice?.reason ?? t("unavailableCurrent"))
      const migrationId = [...current.selected, id].find((selectedId) =>
        current.connectors.some(
          (candidate) =>
            candidate.id === selectedId && candidate.requires_migration
        )
      )
      if (migrationId && !migrationConfirmed) {
        setMigration({ targetId: id })
        return
      }
      const prepared = await prepareConnectorConversation(
        tab.id,
        t("newConversation")
      )
      const result = await setConversationConnectors({
        conversationId: prepared.id,
        revision: current.revision,
        selected: [...new Set([...current.selected, id])],
      })
      prepared.bind()
      route.openConversations()
      toast.success(
        result.migrated.length
          ? t("conversationMigrated")
          : t("conversationEnabled")
      )
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
          key={`${item.id}:${JSON.stringify(item.spec)}`}
          className="flex flex-wrap items-center gap-3 rounded-lg border p-4"
        >
          <div className="min-w-0 flex-1">
            <strong className="text-sm">{item.display_name || item.id}</strong>
            <p className="text-xs text-muted-foreground">
              {t("chooseInConversation")}
            </p>
          </div>
          <ConnectorInstalledStatus item={item} disabled={busy} />
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
      <ConnectorMigrationDialog
        open={migration !== null}
        busy={busy}
        onCancel={() => setMigration(null)}
        onConfirm={() => {
          const pending = migration
          setMigration(null)
          if (pending) void enableConnector(pending.targetId, true)
        }}
      />
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
