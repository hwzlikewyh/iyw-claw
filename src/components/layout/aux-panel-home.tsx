"use client"

import type { ReactNode } from "react"
import { FileDiff, FolderOpen, Globe2, SquareTerminal } from "lucide-react"
import { useTranslations } from "next-intl"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser, waitForHost } from "@/contexts/browser-context"
import { useInteractivePage } from "@/contexts/interactive-page-context"
import { useTerminalContext } from "@/contexts/terminal-context"
import { useWorkspaceActions } from "@/contexts/workspace-context"
import { useBrowserVisibility } from "@/hooks/use-browser-visibility"
import { useIsActiveChatMode } from "@/hooks/use-is-active-chat-mode"
import { isLocalDesktop } from "@/lib/platform"
import { browserApi } from "@/lib/browser-api"
import { DEFAULT_BROWSER_HOME_URL } from "@/lib/browser-defaults"

export function AuxPanelHome() {
  const t = useTranslations("Folder.auxPanel")
  const { activeFolder } = useActiveFolder()
  const isChatMode = useIsActiveChatMode()
  const hasWorkspace = !!activeFolder && !isChatMode
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="shrink-0 px-3 py-4">
        <BrowserHomeAction />
        <WorkspaceHomeAction enabled={hasWorkspace} />
        <ChangesHomeAction
          enabled={hasWorkspace && !!activeFolder?.git_branch}
        />
        <TerminalHomeAction enabled={hasWorkspace} />
        {!hasWorkspace && (
          <p className="px-3 pt-2 text-xs text-muted-foreground">
            {t("noFolderHint")}
          </p>
        )}
      </div>
    </div>
  )
}

function BrowserHomeAction() {
  const t = useTranslations("Folder.auxPanel")
  const { setOpen } = useAuxPanelContext()
  const { state, isOpen, openBrowser, refresh, run } = useBrowser()
  const { closePanel } = useInteractivePage()
  const browserVisible = useBrowserVisibility()
  if (!browserVisible || !isLocalDesktop()) return null
  return (
    <HomeAction
      label={t("openBrowser")}
      icon={<Globe2 className="size-4" />}
      disabled={!state?.capability.supported}
      onClick={() => {
        closePanel()
        setOpen(false)
        void openBrowser().then(() =>
          run(async () => {
            const host = await waitForHost("main", refresh)
            const latest = await browserApi.state()
            const current = latest.hosts.find(
              (item) => item.hostId === host.hostId
            )
            if (!isOpen && current?.tabOrder.length) return latest
            // 首次打开与 BrowserShell 的初始化共享后端互斥，避免多建空白页。
            return !isOpen && latest.tabs.length === 0
              ? browserApi.ensureInitialTab(
                  DEFAULT_BROWSER_HOME_URL,
                  host.hostId
                )
              : browserApi.createTab(DEFAULT_BROWSER_HOME_URL, host.hostId)
          })
        )
      }}
    />
  )
}

function WorkspaceHomeAction({ enabled }: { enabled: boolean }) {
  const t = useTranslations("Folder.auxPanel")
  const { setActiveTab } = useAuxPanelContext()
  return (
    <HomeAction
      label={t("viewWorkspace")}
      icon={<FolderOpen className="size-4" />}
      disabled={!enabled}
      onClick={() => setActiveTab("file_tree")}
    />
  )
}

function ChangesHomeAction({ enabled }: { enabled: boolean }) {
  const t = useTranslations("Folder.auxPanel")
  const { setOpen } = useAuxPanelContext()
  const { closeBrowser } = useBrowser()
  const { closePanel } = useInteractivePage()
  const { openWorkingTreeDiff } = useWorkspaceActions()
  return (
    <HomeAction
      label={t("viewChanges")}
      icon={<FileDiff className="size-4" />}
      disabled={!enabled}
      onClick={() => {
        closePanel()
        void closeBrowser()
        setOpen(false)
        void openWorkingTreeDiff()
      }}
    />
  )
}

function TerminalHomeAction({ enabled }: { enabled: boolean }) {
  const t = useTranslations("Folder.auxPanel")
  const { isOpen, toggle } = useTerminalContext()
  return (
    <HomeAction
      label={t("openTerminal")}
      icon={<SquareTerminal className="size-4" />}
      disabled={!enabled}
      onClick={() => {
        if (!isOpen) toggle()
      }}
    />
  )
}

function HomeAction({
  label,
  icon,
  disabled,
  onClick,
}: {
  label: string
  icon: ReactNode
  disabled?: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex h-11 w-full items-center gap-3 rounded-md px-3 text-left text-sm hover:bg-sidebar-accent focus-visible:outline focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-40"
    >
      <span className="shrink-0 text-muted-foreground" aria-hidden="true">
        {icon}
      </span>
      {label}
    </button>
  )
}
