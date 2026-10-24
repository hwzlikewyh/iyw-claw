"use client"

import {
  createContext,
  useCallback,
  useContext,
  useState,
  type ReactNode,
} from "react"
import { useConnection } from "@/hooks/use-connection"
import { useTabStore } from "@/stores/tab-store"
import type { InteractiveHtmlState } from "@/lib/types"

export type InteractivePage = InteractiveHtmlState
export const LINK_PREVIEW_ID_PREFIX = "link-preview:"
type Selection = {
  scope: string
  id: string | null
  open: boolean
  preview: InteractivePage | null
  external?: { page: InteractivePage; connectionId: string }
}
type Context = {
  isOpen: boolean
  pages: InteractivePage[]
  selectedId: string | null
  connectionId: string | null
  tabId: string | null
  preview: boolean
  select: (id: string) => void
  closePanel: () => void
  openPreview: (page: InteractivePage) => void
  openLivePage: (page: InteractivePage, connectionId: string) => void
}
const PageContext = createContext<Context | null>(null)
const PreviewOpenerContext = createContext<Context["openPreview"] | null>(null)

export function usePagePreviewOpener() {
  return useContext(PreviewOpenerContext)
}

export function useInteractivePage() {
  const context = useContext(PageContext)
  if (!context) throw new Error("InteractivePageProvider is required")
  return context
}

export function InteractivePageProvider({ children }: { children: ReactNode }) {
  const tabId = useTabStore((store) => store.activeTabId)
  const connection = useConnection(tabId ?? "interactive-page-empty")
  const scope = `${tabId}:${connection.connectionId}`
  const [selection, setSelection] = usePageSelection(
    scope,
    connection.interactiveHtml
  )
  const external = useConnection(
    selection.external?.connectionId ?? "interactive-page-external-empty"
  )
  const externalPages = selection.external
    ? selection.external.connectionId === connection.connectionId
      ? connection.interactiveHtml
      : external.interactiveHtml
    : null
  const pages =
    selection.scope === scope && selection.preview
      ? [selection.preview]
      : externalPages
        ? externalPages
        : connection.interactiveHtml
  const requestedId = selection.id
  // 链接只订阅稳定的打开动作，避免跟随会话流式状态重渲染。
  const openPreview = useCallback(
    (page: InteractivePage) =>
      setSelection({
        scope,
        id: page.interaction_id,
        open: true,
        preview: page,
      }),
    [scope, setSelection]
  )
  const selectedId = pages.some((page) => page.interaction_id === requestedId)
    ? requestedId
    : (pages[pages.length - 1]?.interaction_id ?? null)
  const value: Context = {
    isOpen: selection.scope === scope && selection.open && pages.length > 0,
    pages,
    selectedId,
    connectionId: selection.external?.connectionId ?? connection.connectionId,
    tabId,
    preview: Boolean(selection.preview),
    ...selectionActions(scope, selection, setSelection),
    openPreview,
  }
  return (
    <PreviewOpenerContext.Provider value={openPreview}>
      <PageContext.Provider value={value}>{children}</PageContext.Provider>
    </PreviewOpenerContext.Provider>
  )
}

function usePageSelection(scope: string, pages: InteractivePage[]) {
  const [selection, setSelection] = useState<Selection>({
    scope,
    id: null,
    open: false,
    preview: null,
  })
  const pageIds = pages.map((page) => page.interaction_id).join(",")
  const [seen, setSeen] = useState({ scope: "", pageIds: "" })
  if (seen.scope !== scope || seen.pageIds !== pageIds) {
    setSeen({ scope, pageIds })
    const added = pages.filter(
      (page) =>
        seen.scope !== scope ||
        !seen.pageIds.split(",").includes(page.interaction_id)
    )
    const newest = added[added.length - 1] ?? pages[pages.length - 1]
    const selectedExists = pages.some(
      (page) => page.interaction_id === selection.id
    )
    setSelection({
      scope,
      id: added.length
        ? (newest?.interaction_id ?? null)
        : selectedExists
          ? selection.id
          : (newest?.interaction_id ?? null),
      open: added.length ? true : seen.scope === scope && selection.open,
      preview: seen.scope === scope && !added.length ? selection.preview : null,
    })
  }
  return [selection, setSelection] as const
}

function selectionActions(
  scope: string,
  selection: Selection,
  setSelection: React.Dispatch<React.SetStateAction<Selection>>
): Pick<Context, "select" | "closePanel" | "openLivePage"> {
  return {
    select: (id) =>
      setSelection({ ...selection, scope, id, open: true, preview: null }),
    closePanel: () => setSelection({ ...selection, open: false }),
    openLivePage: (page, connectionId) =>
      setSelection({
        scope,
        id: page.interaction_id,
        open: true,
        preview: null,
        external: { page, connectionId },
      }),
  }
}
