import type { OpenedTab } from "./types"

export interface SyncableTab {
  id: string
  conversationId: number | null
}

interface RemoteMergeOptions<T extends SyncableTab> {
  snapshot: OpenedTab[]
  previousTabs: T[]
  previousActiveId: string | null
  ancestorKeys: ReadonlySet<string>
  itemKey: (item: OpenedTab) => string
  tabKey: (tab: T) => string | null
  materialize: (item: OpenedTab, previousTabs: T[]) => T
  retainDraft: (tab: T) => boolean
  createReplacement: () => T | null
}

export interface RemoteMergeResult<T extends SyncableTab> {
  tabs: T[]
  activeTabId: string | null
  snapshotKeys: Set<string>
  diverged: boolean
}

function classifyRemoteItems<T extends SyncableTab>(
  options: RemoteMergeOptions<T>,
  snapshotItems: OpenedTab[],
  snapshotKeys: ReadonlySet<string>
): { remoteItems: OpenedTab[]; localAdditions: T[]; diverged: boolean } {
  const localKeys = new Set(
    options.previousTabs
      .map(options.tabKey)
      .filter((key): key is string => key != null)
  )
  const remoteItems = snapshotItems.filter((item) => {
    const key = options.itemKey(item)
    return localKeys.has(key) || !options.ancestorKeys.has(key)
  })
  const localAdditions = options.previousTabs.filter((tab) => {
    const key = options.tabKey(tab)
    return !!key && !snapshotKeys.has(key) && !options.ancestorKeys.has(key)
  })
  return {
    remoteItems,
    localAdditions,
    diverged:
      localAdditions.length > 0 || remoteItems.length !== snapshotItems.length,
  }
}

function resolveFocus<T extends SyncableTab>(
  options: RemoteMergeOptions<T>,
  context: {
    tabs: T[]
    remoteItems: OpenedTab[]
    snapshotKeys: ReadonlySet<string>
  }
): string | null {
  const { tabs, remoteItems, snapshotKeys } = context
  const remoteActive = remoteItems.find((item) => item.is_active)
  const remoteActiveId = remoteActive
    ? tabs.find((tab) => options.tabKey(tab) === options.itemKey(remoteActive))
        ?.id
    : null
  const active = tabs.find((tab) => tab.id === options.previousActiveId)
  const activeKey = active ? options.tabKey(active) : null
  const localOnly =
    active &&
    (activeKey == null ||
      (!snapshotKeys.has(activeKey) && !options.ancestorKeys.has(activeKey)))
  if (localOnly) return options.previousActiveId
  return remoteActiveId ?? active?.id ?? tabs[0]?.id ?? null
}

export function mergeRemoteTabSnapshot<T extends SyncableTab>(
  options: RemoteMergeOptions<T>
): RemoteMergeResult<T> {
  const snapshotItems = options.snapshot.filter(
    (item) => item.conversation_id != null
  )
  const snapshotKeys = new Set(snapshotItems.map(options.itemKey))
  const classified = classifyRemoteItems(options, snapshotItems, snapshotKeys)
  const remoteTabs = classified.remoteItems.map((item) =>
    options.materialize(item, options.previousTabs)
  )
  let tabs = [
    ...remoteTabs,
    ...classified.localAdditions,
    ...options.previousTabs.filter(
      (tab) => tab.conversationId == null && options.retainDraft(tab)
    ),
  ]
  if (tabs.length === 0) {
    const replacement = options.createReplacement()
    if (replacement) tabs = [replacement]
  }
  return {
    tabs,
    activeTabId: resolveFocus(options, {
      tabs,
      remoteItems: classified.remoteItems,
      snapshotKeys,
    }),
    snapshotKeys,
    diverged: classified.diverged,
  }
}
