"use client"

import { useState } from "react"
import { toast } from "sonner"
import { useTranslations } from "next-intl"
import { Globe, PanelRightClose, X } from "lucide-react"
import {
  useInteractivePage,
  LINK_PREVIEW_ID_PREFIX,
  type InteractivePage,
} from "@/contexts/interactive-page-context"
import { Button } from "@/components/ui/button"
import { InteractiveHtmlCard } from "./interactive-html-card"
import { NativePageView } from "./native-page-view"
const MAX_WEB_TABS = 8

export function InteractivePagePanel() {
  const context = useInteractivePage()
  const t = useTranslations("Folder.chat.interactiveHtml")
  return (
    <aside
      className="flex h-full min-h-0 flex-col overflow-hidden bg-background"
      aria-label={t("title")}
    >
      <PagePanelHeader />
      {context.preview &&
        !context.selectedId?.startsWith(LINK_PREVIEW_ID_PREFIX) && (
          <p className="border-b p-2 text-xs text-muted-foreground">
            {t("historyNotice")}
          </p>
        )}
      {context.pages.map((page) => (
        <div
          key={page.interaction_id}
          className={
            page.interaction_id === context.selectedId
              ? "min-h-0 flex-1"
              : "hidden"
          }
        >
          {page.url ? (
            <UrlPage
              page={page}
              visible={
                context.isOpen && page.interaction_id === context.selectedId
              }
              tabId={context.tabId ?? ""}
            />
          ) : (
            <InteractiveHtmlCard
              page={page}
              connectionId={context.connectionId}
              preview={context.preview}
              panel
            />
          )}
        </div>
      ))}
    </aside>
  )
}

function UrlPage({
  page,
  visible,
  tabId,
}: {
  page: InteractivePage
  visible: boolean
  tabId: string
}) {
  const [tabs, setTabs] = useState([
    { id: crypto.randomUUID(), url: page.url! },
  ])
  const [active, setActive] = useState(tabs[0].id)
  const t = useTranslations("Folder.chat.interactiveHtml")
  const context = useInteractivePage()
  return (
    <div className="flex h-full min-h-0 flex-col">
      <UrlTabStrip
        tabs={tabs}
        active={active}
        activate={setActive}
        close={(id) => {
          const next = tabs.filter((tab) => tab.id !== id)
          setTabs(next)
          if (active === id) setActive(next[0].id)
        }}
      />
      <UrlTabContent
        tabs={tabs}
        active={active}
        visible={visible}
        tabId={tabId}
        onNewTab={(url) => {
          if (tabs.length >= MAX_WEB_TABS) {
            toast.error(t("native.tabLimit"))
            return
          }
          const id = crypto.randomUUID()
          setTabs([...tabs, { id, url }])
          setActive(id)
        }}
      />
      <UrlResponse
        page={page}
        connectionId={context.connectionId}
        preview={context.preview}
      />
    </div>
  )
}

function UrlResponse({
  page,
  connectionId,
  preview,
}: {
  page: InteractivePage
  connectionId: string | null
  preview: boolean
}) {
  if (page.interaction_id.startsWith(LINK_PREVIEW_ID_PREFIX)) return null
  return (
    <InteractiveHtmlCard
      page={page}
      connectionId={connectionId}
      preview={preview}
      responseOnly
    />
  )
}

function PagePanelHeader() {
  const context = useInteractivePage()
  const t = useTranslations("Folder.chat.interactiveHtml")
  return (
    <div className="flex shrink-0 items-center gap-1 border-b px-2 py-1">
      <select
        value={context.selectedId ?? ""}
        aria-label={t("title")}
        onChange={(event) => context.select(event.target.value)}
        className="h-8 min-w-0 flex-1 bg-background text-sm"
      >
        {context.pages.map((page) => (
          <option key={page.interaction_id} value={page.interaction_id}>
            {page.title}
          </option>
        ))}
      </select>
      <Button
        size="icon-sm"
        variant="ghost"
        aria-label={t("close")}
        title={t("close")}
        onClick={context.closePanel}
      >
        <PanelRightClose />
      </Button>
    </div>
  )
}

function UrlTabStrip({
  tabs,
  active,
  activate,
  close,
}: {
  tabs: { id: string; url: string }[]
  active: string
  activate: (id: string) => void
  close: (id: string) => void
}) {
  const t = useTranslations("Folder.chat.interactiveHtml")
  return (
    <div className="flex shrink-0 overflow-auto border-b">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className={`flex min-w-0 items-center gap-1 border-r p-1 text-xs ${active === tab.id ? "bg-muted" : ""}`}
        >
          <button
            className="max-w-40 truncate px-2"
            onClick={() => activate(tab.id)}
            title={tab.url}
          >
            <Globe className="mr-1 inline size-3" />
            {tab.url}
          </button>
          {tabs.length > 1 && (
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={t("close")}
              onClick={() => close(tab.id)}
            >
              <X />
            </Button>
          )}
        </div>
      ))}
    </div>
  )
}

function UrlTabContent({
  tabs,
  active,
  visible,
  tabId,
  onNewTab,
}: {
  tabs: { id: string; url: string }[]
  active: string
  visible: boolean
  tabId: string
  onNewTab: (url: string) => void
}) {
  return (
    <>
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className={active === tab.id ? "min-h-0 flex-1" : "hidden"}
        >
          <NativePageView
            id={tab.id}
            url={tab.url}
            visible={visible && active === tab.id}
            tabId={tabId}
            onNewTab={onNewTab}
          />
        </div>
      ))}
    </>
  )
}
