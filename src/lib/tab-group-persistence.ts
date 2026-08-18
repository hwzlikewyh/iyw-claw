import {
  firstLeafId,
  isLayoutNode,
  normalizeLayout,
  singleGroupLayout,
  type LayoutNode,
} from "./tab-group-layout"

const STORAGE_PREFIX = "workspace:tab-groups:v1"
const LEGACY_TILE_KEY = "workspace:tile-mode"

export interface PersistedGroupDraft {
  id: string
  group: string
  index: number
  folderId: number
  isChat?: boolean
  workingDir?: string
  agentType?: string
}

export interface TabGroupSnapshot {
  layout: LayoutNode
  assignments: Record<string, string>
  selection: Record<string, string>
  tileByGroup: Record<string, boolean>
  drafts: PersistedGroupDraft[]
  activeDraft: string | null
}

function storageKey(): string {
  if (typeof window === "undefined") return `${STORAGE_PREFIX}:local`
  const remoteId = new URLSearchParams(window.location.search).get(
    "remoteConnectionId"
  )
  return `${STORAGE_PREFIX}:${remoteId ? `remote-${remoteId}` : "local"}`
}

function stringRecord(value: unknown): Record<string, string> {
  if (typeof value !== "object" || value === null) return {}
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>).filter(
      (entry): entry is [string, string] => typeof entry[1] === "string"
    )
  )
}

function boolRecord(value: unknown): Record<string, boolean> {
  if (typeof value !== "object" || value === null) return {}
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>).filter(
      (entry): entry is [string, boolean] => typeof entry[1] === "boolean"
    )
  )
}

function sanitizeDraft(value: unknown): PersistedGroupDraft | null {
  if (typeof value !== "object" || value === null) return null
  const draft = value as Record<string, unknown>
  if (
    typeof draft.id !== "string" ||
    draft.id.length === 0 ||
    typeof draft.group !== "string" ||
    draft.group.length === 0
  )
    return null
  if (typeof draft.folderId !== "number" || !Number.isFinite(draft.folderId)) {
    return null
  }
  const index =
    typeof draft.index === "number" && Number.isFinite(draft.index)
      ? Math.max(0, Math.floor(draft.index))
      : 0
  return {
    id: draft.id,
    group: draft.group,
    index,
    folderId: draft.folderId,
    ...(draft.isChat === true ? { isChat: true } : {}),
    ...(typeof draft.workingDir === "string" && draft.workingDir.length > 0
      ? { workingDir: draft.workingDir }
      : {}),
    ...(typeof draft.agentType === "string" && draft.agentType.length > 0
      ? { agentType: draft.agentType }
      : {}),
  }
}

function fallbackSnapshot(): TabGroupSnapshot {
  const layout = singleGroupLayout()
  const tileByGroup: Record<string, boolean> = {}
  if (typeof window !== "undefined") {
    try {
      if (localStorage.getItem(LEGACY_TILE_KEY) === "true") {
        tileByGroup[firstLeafId(layout)] = true
      }
    } catch {
      // Storage may be unavailable in hardened webviews.
    }
  }
  return {
    layout,
    assignments: {},
    selection: {},
    tileByGroup,
    drafts: [],
    activeDraft: null,
  }
}

function parseSnapshot(raw: string | null): TabGroupSnapshot | null {
  if (!raw) return null
  try {
    const value = JSON.parse(raw) as Record<string, unknown>
    if (!isLayoutNode(value.layout)) return null
    const drafts = Array.isArray(value.drafts)
      ? value.drafts.map(sanitizeDraft).filter((draft) => draft !== null)
      : []
    return {
      layout: normalizeLayout(value.layout),
      assignments: stringRecord(value.assignments),
      selection: stringRecord(value.selection),
      tileByGroup: boolRecord(value.tileByGroup),
      drafts,
      activeDraft:
        typeof value.activeDraft === "string" ? value.activeDraft : null,
    }
  } catch {
    return null
  }
}

export function loadTabGroupSnapshot(): TabGroupSnapshot {
  if (typeof window === "undefined") return fallbackSnapshot()
  try {
    const scoped = parseSnapshot(localStorage.getItem(storageKey()))
    if (scoped) return scoped
    const legacy = parseSnapshot(localStorage.getItem(STORAGE_PREFIX))
    return legacy ?? fallbackSnapshot()
  } catch {
    return fallbackSnapshot()
  }
}

export function saveTabGroupSnapshot(
  snapshot: TabGroupSnapshot
): string | null {
  if (typeof window === "undefined") return null
  const blob = JSON.stringify(snapshot)
  try {
    localStorage.setItem(storageKey(), blob)
    return blob
  } catch {
    return null
  }
}
