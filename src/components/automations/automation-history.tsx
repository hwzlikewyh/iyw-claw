"use client"

import { useState } from "react"
import { ArrowLeft, ArrowRight, Loader2, RotateCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import {
  automationCancelRun,
  automationRetryRun,
  automationRetryRuns,
  getFolderConversation,
  automationGetRun,
} from "@/lib/api"
import { useTabActions } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { Button } from "@/components/ui/button"
import type { Automation, AutomationRun } from "@/lib/types"
import {
  AutomationRunFilters,
  type RunFiltersValue,
} from "./automation-run-filters"
import { AutomationRunActions } from "./automation-run-actions"
import { AutomationRetryDialog } from "./automation-retry-dialog"
import { RUN_PAGE_SIZE, useAutomationHistory } from "./use-automation-history"
import { AutomationHistoryRows } from "./automation-history-rows"
import { AutomationRelatedRun } from "./automation-related-run"
import { useAutomationErrorMessage } from "./automation-run-error"
import styles from "./automation-workspace.module.css"

export function AutomationHistory({
  automations,
  automation,
  onSelectTask,
  onEditTask,
  onChanged,
  initialStatus = "all",
}: {
  automations: Automation[]
  automation?: Automation
  onSelectTask: (task: Automation) => void
  onEditTask: (task: Automation) => void
  onChanged: () => Promise<void>
  initialStatus?: string
}) {
  const t = useTranslations("Automations")
  const errorMessage = useAutomationErrorMessage()
  const { openTab } = useTabActions()
  const { openConversations } = useWorkbenchRoute()
  const [filters, setFilters] = useState<RunFiltersValue>({
    folder: "all",
    status: initialStatus,
    trigger: "all",
    task: "all",
    search: "",
  })
  const [offset, setOffset] = useState(0)
  const [busy, setBusy] = useState(false)
  const [relatedRun, setRelatedRun] = useState<AutomationRun | null>(null)
  const [retry, setRetry] = useState<{
    runs: AutomationRun[]
    batch: boolean
  } | null>(null)
  const query = {
    folderId: filters.folder === "all" ? undefined : Number(filters.folder),
    automationId:
      automation?.id ??
      (filters.task === "all" ? undefined : Number(filters.task)),
    status: filters.status === "all" ? undefined : filters.status,
    trigger: filters.trigger === "all" ? undefined : filters.trigger,
    search: filters.search,
    offset,
  }
  const { runs, total, loading, error, reload } = useAutomationHistory(query)
  const taskFor = (run: AutomationRun) =>
    automations.find((task) => task.id === run.automation_id)
  const changeFilters = (value: RunFiltersValue) => {
    setFilters(value)
    setOffset(0)
  }
  const perform = async (action: () => Promise<unknown>) => {
    setBusy(true)
    try {
      await action()
      await reload()
      await onChanged()
    } catch (cause) {
      toast.error(errorMessage(cause))
    } finally {
      setBusy(false)
    }
  }
  const openConversation = async (id: number) => {
    try {
      const { summary } = await getFolderConversation(id)
      openConversations()
      openTab(summary.folder_id, id, summary.agent_type)
    } catch (cause) {
      toast.error(String(cause))
    }
  }
  const related = (id: number) => {
    void automationGetRun(id)
      .then((linked) => {
        if (linked.conversation_id != null)
          void openConversation(linked.conversation_id)
        else setRelatedRun(linked)
      })
      .catch((cause) => toast.error(String(cause)))
  }
  const actions = (run: AutomationRun) => (
    <AutomationRunActions
      run={run}
      busy={busy}
      active={taskFor(run)?.last_run_status === "running"}
      onRetry={() => setRetry({ runs: [run], batch: false })}
      onCancel={() => void perform(() => automationCancelRun(run.id))}
      onEdit={taskFor(run) ? () => onEditTask(taskFor(run)!) : undefined}
      onRelated={related}
      onOpen={
        run.conversation_id != null
          ? () => void openConversation(run.conversation_id!)
          : undefined
      }
    />
  )
  const confirmRetry = (ids: number[]) =>
    void perform(async () => {
      if (retry?.batch) {
        const count = await automationRetryRuns(ids)
        toast.success(t("retryBatchQueued", { count }))
      } else {
        await automationRetryRun(ids[0])
        toast.success(t("retryStarted"))
      }
      setRetry(null)
    })
  const eligible = runs.filter(
    (run) =>
      run.status === "failed" &&
      run.retry_run_id == null &&
      taskFor(run)?.last_run_status !== "running"
  )
  return (
    <section>
      <div className="mb-5 flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-semibold">
          {t("runsTab")}{" "}
          <span className="ml-2 text-xs font-normal text-muted-foreground">
            {t("runCount", { count: total })}
          </span>
        </h2>
        <div className="flex flex-wrap items-center gap-2">
          <AutomationRunFilters
            value={filters}
            onChange={changeFilters}
            automations={automations}
            single={!!automation}
          />
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={t("refresh")}
            onClick={() => void reload()}
          >
            <RotateCw className="size-3.5" />
          </Button>
          {!automation && (
            <Button
              size="sm"
              variant="outline"
              disabled={busy || eligible.length === 0}
              onClick={() => setRetry({ runs: eligible, batch: true })}
            >
              <RotateCw className="size-3.5" />
              {t("retryBatch")}
            </Button>
          )}
        </div>
      </div>
      {error && (
        <div
          role="alert"
          className="mb-4 flex flex-wrap items-center gap-2 text-xs text-destructive"
        >
          {t("loadError")}: {error}
          <Button size="sm" variant="outline" onClick={() => void reload()}>
            {t("retry")}
          </Button>
        </div>
      )}
      {loading && !runs.length ? (
        <div className={styles.empty}>
          <Loader2 className="mx-auto size-5 animate-spin" />
        </div>
      ) : !runs.length ? (
        <p className={styles.empty}>{t("noRuns")}</p>
      ) : (
        <AutomationHistoryRows
          runs={runs}
          automation={automation}
          taskFor={taskFor}
          onSelectTask={onSelectTask}
          actions={actions}
        />
      )}
      <div className="mt-5 flex flex-wrap items-center justify-between gap-3 text-[11px] text-muted-foreground">
        <span>
          {t("historyPage", {
            start: total ? offset + 1 : 0,
            end: Math.min(offset + runs.length, total),
            total,
          })}
        </span>
        <div className="flex gap-2">
          <Button
            variant="ghost"
            size="sm"
            disabled={loading || offset === 0}
            onClick={() => setOffset(Math.max(0, offset - RUN_PAGE_SIZE))}
          >
            <ArrowLeft className="size-3" />
            {t("previousPage")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={loading || offset + runs.length >= total}
            onClick={() => setOffset(offset + RUN_PAGE_SIZE)}
          >
            {t("nextPage")}
            <ArrowRight className="size-3" />
          </Button>
        </div>
      </div>
      {relatedRun && (
        <AutomationRelatedRun
          run={relatedRun}
          onClose={() => setRelatedRun(null)}
        />
      )}
      {retry && (
        <AutomationRetryDialog
          key={retry.runs.map((run) => run.id).join(",")}
          {...retry}
          busy={busy}
          onClose={() => setRetry(null)}
          onConfirm={confirmRetry}
        />
      )}
    </section>
  )
}
