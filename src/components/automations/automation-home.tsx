"use client"
import { CalendarClock, Clock, Plus, Search } from "lucide-react"
import { automationRunNow, automationSetEnabled } from "@/lib/api"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { AutomationTaskCard } from "./automation-task-card"
import { AutomationHistory } from "./automation-history"
import type { AutomationWorkspace } from "./use-automation-workspace"
import styles from "./automation-workspace.module.css"

export function AutomationHome({
  workspace,
}: {
  workspace: AutomationWorkspace
}) {
  const {
    t,
    automations,
    folders,
    channels,
    view,
    setView,
    busy,
    search,
    setSearch,
    folder,
    setFolder,
    filter,
    setFilter,
    runStatus,
    setRunStatus,
    perform,
    selectTask,
    editTask,
    setNotify,
    folderName,
    visible,
    pending,
    refetch,
  } = workspace
  return (
    <>
      {(view === "tasks" || view === "runs") && (
        <>
          <header className={styles.header}>
            <nav className={styles.tabs} aria-label={t("title")}>
              <button
                className={styles.tab}
                aria-current={view === "tasks" ? "page" : undefined}
                onClick={() => setView("tasks")}
              >
                <CalendarClock className="size-4" />
                {t("tasksTab")}
                <span className={styles.tabCount}>{automations.length}</span>
              </button>
              <button
                className={styles.tab}
                aria-current={view === "runs" ? "page" : undefined}
                onClick={() => {
                  setRunStatus("all")
                  setView("runs")
                }}
              >
                <Clock className="size-4" />
                {t("runsTab")}
              </button>
            </nav>
            <Button size="sm" onClick={() => setView("gallery")}>
              <Plus className="size-3.5" />
              {t("new")}
            </Button>
          </header>
          {view === "tasks" ? (
            <>
              <div className={styles.toolbar}>
                <div className="flex flex-wrap items-center gap-4">
                  <div className={styles.segments}>
                    {[
                      ["all", t("filterAll"), automations.length],
                      [
                        "enabled",
                        t("enabled"),
                        automations.filter((task) => task.enabled).length,
                      ],
                      [
                        "disabled",
                        t("statusDisabled"),
                        automations.filter((task) => !task.enabled).length,
                      ],
                    ].map(([id, label, count]) => (
                      <button
                        key={id}
                        className={styles.segment}
                        aria-pressed={filter === id}
                        onClick={() => setFilter(String(id))}
                      >
                        {label}
                        <span className="ml-1.5 text-[10px] opacity-60">
                          {count}
                        </span>
                      </button>
                    ))}
                  </div>
                  {pending > 0 && (
                    <button
                      className="text-[11px] text-amber-600 dark:text-amber-400"
                      onClick={() => {
                        setRunStatus("failed")
                        setView("runs")
                      }}
                    >
                      {t("pendingRuns", { count: pending })}
                    </button>
                  )}
                </div>
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <label className="flex h-8 items-center gap-2 rounded-md border border-border bg-background px-2.5">
                    <Search className="size-3.5 text-muted-foreground" />
                    <input
                      type="search"
                      className="w-36 min-w-0 bg-transparent text-xs outline-none"
                      value={search}
                      onChange={(event) => setSearch(event.target.value)}
                      aria-label={t("searchTasks")}
                      placeholder={t("searchTasks")}
                    />
                  </label>
                  <Select value={folder} onValueChange={setFolder}>
                    <SelectTrigger
                      size="sm"
                      className="max-w-48 text-xs"
                      aria-label={t("allFolders")}
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">{t("allFolders")}</SelectItem>
                      {folders.map((item) => (
                        <SelectItem key={item.id} value={String(item.id)}>
                          {item.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <section className={styles.grid} aria-label={t("tasksTab")}>
                {visible.map((task) => (
                  <AutomationTaskCard
                    key={task.id}
                    automation={task}
                    folder={folderName(task)}
                    channels={channels}
                    busy={busy}
                    onSelect={() => selectTask(task)}
                    onRun={() => void perform(() => automationRunNow(task.id))}
                    onEdit={() => editTask(task)}
                    onToggle={() =>
                      void perform(() =>
                        automationSetEnabled(task.id, !task.enabled)
                      )
                    }
                    onNotify={() => setNotify(task)}
                  />
                ))}
                {!visible.length && (
                  <div className={styles.empty}>
                    <p>{automations.length ? t("noMatches") : t("empty")}</p>
                    {!automations.length && (
                      <>
                        <p className="my-3 text-xs">{t("firstRunHint")}</p>
                        <Button size="sm" onClick={() => setView("gallery")}>
                          {t("new")}
                        </Button>
                      </>
                    )}
                  </div>
                )}
              </section>
            </>
          ) : (
            <AutomationHistory
              key={runStatus}
              initialStatus={runStatus}
              automations={automations}
              onSelectTask={selectTask}
              onEditTask={editTask}
              onChanged={refetch}
            />
          )}
        </>
      )}
    </>
  )
}
