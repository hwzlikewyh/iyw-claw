"use client"

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react"
import {
  Copy,
  Download,
  FileCode,
  FileImage,
  FileText,
  Info,
  RefreshCw,
  SquarePen,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useAcpActions, useAcpEvent } from "@/contexts/acp-connections-context"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import {
  useTabActions,
  useTabStore,
  type TabItem,
} from "@/contexts/tab-context"
import { groupOfTab, isReparentUnmount } from "@/stores/tab-store"
import { useSessionStats } from "@/contexts/session-stats-context"
import { useTaskContext } from "@/contexts/task-context"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { cn, copyTextFromMenu, randomUUID } from "@/lib/utils"
import { useConnectionLifecycle } from "@/hooks/use-connection-lifecycle"
import { useDocumentVisibility } from "@/hooks/use-document-visibility"
import { useIsMobile } from "@/hooks/use-mobile"
import { useMessageQueue, type QueuedMessage } from "@/hooks/use-message-queue"
import { useSortedAvailableAgents } from "@/hooks/use-sorted-available-agents"
import { MessageListView } from "@/components/message/message-list-view"
import type { MessageScrollPosition } from "@/components/message/virtualized-message-thread"
import { ConversationShell } from "@/components/chat/conversation-shell"
import { SessionConfigStaleBanner } from "@/components/chat/session-config-stale-banner"
import { BackgroundTasksChip } from "@/components/chat/background-tasks-chip"
import { FeedbackNotesDisplay } from "@/components/chat/feedback-notes-display"
import { FeedbackDialog } from "@/components/chat/feedback-dialog"
import { useFeedbackEnabled } from "@/hooks/use-feedback-enabled"
import { useSessionFeedback } from "@/hooks/use-session-feedback"
import { AgentSelector } from "@/components/chat/agent-selector"
import { ChatInput } from "@/components/chat/chat-input"
import { WelcomeHero } from "@/components/chat/welcome-hero"
import { QuickActions } from "@/components/chat/quick-actions"
import {
  ConversationPointsDialog,
  getConversationPointsBlockReason,
  type ConversationPointsBlockReason,
} from "@/components/conversations/conversation-points-gate"
import type { ComposerInjectContent } from "@/components/chat/message-input"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  acpFork,
  createChatConversation,
  createChatDir,
  createConversation,
  deleteAgentInput,
  forceAgentInputsThrough,
  getConversationContextPrimer,
  openSettingsWindow,
  queueAgentInput,
  reorderAgentInputs,
  retryAgentInput,
  skillActivationSet,
  skillInventoryList,
  submitAgentInput,
} from "@/lib/api"
import { skillMarketInstall } from "@/lib/skill-market"
import {
  flushRetryDelayMs,
  forkSendBlockedByQueue,
  canConnectionAcceptPrompt,
  shouldBlockUnboundSend,
  shouldQueueBeforeConnection,
  shouldQueueDirectSend,
  shouldRejectDuplicateCreate,
} from "@/lib/queue-flush"
import { TurnBusyError } from "@/lib/turn-busy"
import {
  getConversationIdByExternalIdFromStore,
  getRuntimeSession,
  getTimelineTurns,
  useConversationRuntimeActions,
  useConversationRuntimeStore,
} from "@/stores/conversation-runtime-store"
import { useShallow } from "zustand/react/shallow"
import { useConversationDetail } from "@/hooks/use-conversation-detail"
import {
  extractUserImagesFromDraft,
  getPromptDraftDisplayText,
} from "@/lib/prompt-draft"
import {
  type AgentType,
  type ContentBlock,
  type EventEnvelope,
  type MessageTurn,
  type PromptDraft,
  type QuestionAnswer,
  type UserMessageBlock,
  type PromptSkillPackage,
  type SkillInventorySnapshot,
} from "@/lib/types"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import {
  getFixedAgentOptions,
  hasAuthoritativeFixedAgentOptions,
  loadFixedAgentOptions,
  refreshFixedAgentOptions,
} from "@/lib/fixed-agent-options"
import { reconcileModelConfigValues } from "@/lib/gateway-model-catalog"
import {
  currentModelName,
  isModelConfigOption,
} from "@/lib/model-config-groups"
import { planSessionConfigSync } from "@/lib/session-config-compat"
import {
  lastUserPromptText,
  type SessionFailureAction,
} from "@/lib/session-failures"
import { isInsufficientBalanceError } from "@/lib/agent-runtime-error"
import type { SessionConfigTranslator } from "@/lib/session-config-localization"

const MODEL_REAPPLY_TIMEOUT_MS = 45_000

function scenarioPackageReady(
  snapshot: SkillInventorySnapshot,
  packageRef: PromptSkillPackage,
  agentType: AgentType
): boolean {
  const root = snapshot.skills.find((skill) =>
    skill.observations.some(
      (observation) =>
        observation.marketSkillId === packageRef.id &&
        observation.installedVersion === packageRef.version
    )
  )
  if (!root) return false
  const byId = new Map(snapshot.skills.map((skill) => [skill.skillId, skill]))
  const pending = [root.skillId]
  const visited = new Set<string>()
  while (pending.length > 0) {
    const skillId = pending.pop()
    if (!skillId || visited.has(skillId)) continue
    visited.add(skillId)
    const skill = byId.get(skillId)
    if (
      !skill ||
      !skill.agentStates.some(
        (state) => state.agentType === agentType && state.actualEnabled
      )
    ) {
      return false
    }
    pending.push(...skill.dependencies)
  }
  return true
}

function scenarioPackageRoot(
  snapshot: SkillInventorySnapshot,
  packageRef: PromptSkillPackage
) {
  return snapshot.skills.find((skill) =>
    skill.observations.some(
      (observation) =>
        observation.marketSkillId === packageRef.id &&
        observation.installedVersion === packageRef.version
    )
  )
}

function scenarioPackageTargetsAgent(
  snapshot: SkillInventorySnapshot,
  packageRef: PromptSkillPackage,
  agentType: AgentType
): boolean {
  const root = scenarioPackageRoot(snapshot, packageRef)
  return Boolean(
    root?.observations.some((observation) =>
      observation.locations.some((location) =>
        location.agentTypes.includes(agentType)
      )
    )
  )
}

async function prepareScenarioPackage(
  packageRef: PromptSkillPackage,
  agentType: AgentType,
  workspacePath?: string
) {
  let snapshot = await skillInventoryList(workspacePath)
  if (!scenarioPackageTargetsAgent(snapshot, packageRef, agentType)) {
    await skillMarketInstall(packageRef.id, packageRef.version, [agentType])
    snapshot = await skillInventoryList(workspacePath)
  }
  if (!scenarioPackageReady(snapshot, packageRef, agentType)) {
    const root = scenarioPackageRoot(snapshot, packageRef)
    if (!root) throw new Error("技能包安装记录不存在")
    const result = await skillActivationSet({
      skillId: root.skillId,
      scope: root.scope,
      workspacePath,
      agentType,
      enabled: true,
      expectedRevision: snapshot.revision,
    })
    if (result.error) throw new Error(result.error)
    snapshot = await skillInventoryList(workspacePath)
  }
  if (!scenarioPackageReady(snapshot, packageRef, agentType)) {
    throw new Error("技能包或依赖未能对当前 Agent 启用")
  }
}
import {
  getSavedModeId,
  getSavedPrefsForConnect,
  replaceConfigPreferences,
  saveConfigPreference,
  saveModePreference,
} from "@/lib/selector-prefs-storage"
import {
  adoptLegacyNewConversationDraft,
  buildConversationDraftStorageKey,
  buildNewConversationDraftStorageKey,
  clearMessageInputDraft,
  saveMessageInputDraft,
  subscribeMessageInputDraftPresence,
} from "@/lib/message-input-draft"
import { computeRects, leafIds, type GroupRect } from "@/lib/tab-group-layout"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  exportAsHtml,
  exportAsImage,
  exportAsMarkdown,
  ExportTooLongError,
  type ExportLabels,
} from "@/lib/export-conversation"
import { resolveActiveSessionDetails } from "./active-session-details"
import { SessionDetailsDialog } from "./session-details-dialog"
import { GroupSplitHandle } from "./group-split-handle"
import { TabBar } from "@/components/tabs/tab-bar"

interface ConversationTabViewProps {
  tabId: string
  conversationId: number | null
  agentType: AgentType
  workingDir?: string
  isActive: boolean
  isVisible: boolean
  /** Drive the composer's flowing active-session border. True only for the
   *  active tab while tiled across multiple sessions — the one place the flow
   *  serves as the "which tile is active" cue. Distinct from `isActive`, which
   *  also governs auto-focus/connect and is true even for a lone session. */
  showActiveFlow: boolean
  reloadSignal: number
  groupId: string
}

interface PendingBalanceRecovery {
  connectionId: string
  draft: PromptDraft
  modeId: string | null
  requeued: boolean
}

function buildOptimisticUserTurnFromDraft(
  draft: PromptDraft,
  attachedResourcesFallback: string,
  messageId?: string
): MessageTurn {
  // `draft.displayText` is the composer's full Markdown, which already renders
  // every inline file/resource badge as a `[label](uri)` link (see
  // `referenceToMarkdown`). Re-appending the resource blocks here would duplicate
  // each attached file in the optimistic bubble, so the display text is used
  // as-is — images are the only out-of-band content left to add as blocks.
  const images = extractUserImagesFromDraft(draft)
  const imageUris = new Set(
    images
      .map((image) => image.uri?.trim())
      .filter((uri): uri is string => Boolean(uri))
  )
  const hasVisibleNonImageContent = draft.blocks.some((block) => {
    if (block.type === "text") return block.text.trim().length > 0
    if (block.type === "resource") return true
    return block.type === "resource_link" && !imageUris.has(block.uri.trim())
  })
  const text = hasVisibleNonImageContent
    ? getPromptDraftDisplayText(draft, attachedResourcesFallback)
    : ""

  const blocks: ContentBlock[] = []
  for (const image of images) {
    blocks.push({
      type: "image",
      data: image.data,
      mime_type: image.mime_type,
      uri: image.uri ?? null,
    })
  }
  if (text) blocks.push({ type: "text", text })

  return {
    id: messageId ?? `optimistic-${randomUUID()}`,
    role: "user",
    blocks,
    timestamp: new Date().toISOString(),
  }
}

/** Build a user `MessageTurn` from a broadcast `user_message` (event or
 *  snapshot `pending_user_message`). Used by cross-client VIEWERS to render the
 *  sender's prompt. The turn `id` is the broadcast `message_id` so the runtime
 *  reducer can dedup it idempotently. */
function buildUserTurnFromMessageBlocks(
  messageId: string,
  blocks: UserMessageBlock[]
): MessageTurn {
  const contentBlocks: ContentBlock[] = blocks.map((b) =>
    b.type === "image"
      ? {
          type: "image",
          data: b.data,
          mime_type: b.mime_type,
          uri: b.uri ?? null,
        }
      : { type: "text", text: b.text }
  )
  return {
    id: messageId,
    role: "user",
    blocks: contentBlocks,
    timestamp: new Date().toISOString(),
  }
}

function buildVirtualConversationId(seed: string): number {
  let hash = 0
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) | 0
  }
  const normalized = Math.abs(hash) + 1
  return -normalized
}

const ConversationTabView = memo(function ConversationTabView({
  tabId,
  conversationId,
  agentType,
  workingDir,
  isActive,
  isVisible,
  showActiveFlow,
  reloadSignal,
  groupId,
}: ConversationTabViewProps) {
  const isDocumentVisible = useDocumentVisibility()
  const { status: accountStatus, profile: accountProfile } = useIywAccount()
  const [pointsDialogReason, setPointsDialogReason] =
    useState<ConversationPointsBlockReason | null>(null)
  const currentPointsBlockReason = getConversationPointsBlockReason(
    accountStatus,
    accountProfile?.balance_points
  )
  const ensureConversationPointsAvailable = useCallback(() => {
    if (currentPointsBlockReason === null) return true
    console.warn("[conversation-points] blocked prompt submission", {
      tabId,
      accountStatus,
      reason: currentPointsBlockReason,
    })
    setPointsDialogReason(currentPointsBlockReason)
    return false
  }, [accountStatus, currentPointsBlockReason, tabId])
  useEffect(() => {
    if (currentPointsBlockReason === null) setPointsDialogReason(null)
  }, [currentPointsBlockReason])
  const [catalogVersion, setCatalogVersion] = useState(0)
  const t = useTranslations("Folder.conversation")
  const tWelcome = useTranslations("Folder.chat.welcomeInputPanel")
  const tAgentInput = useTranslations("Folder.chat.agentInput")
  const tAutoContinuation = useTranslations(
    "Folder.chat.acpConnections.autoContinuation"
  )
  const sharedT = useTranslations("Folder.chat.shared")
  const tConfig = useTranslations("Folder.chat.messageInput")
  const tSessionFailure = useTranslations("Folder.chat.sessionFailure")
  const tConfigStale = useTranslations("Folder.chat.configStale")
  const refreshConversations = useAppWorkspaceStore(
    (s) => s.refreshConversations
  )
  const upsertFolder = useAppWorkspaceStore((s) => s.upsertFolder)
  // Subscribe to ONLY this tab's own row (identified by `tabId`), not the whole
  // `tabs` array — so a sibling tab changing, or a tab-switch (isActive rides in
  // as a prop), never re-renders this keep-alive panel. `find` returns the same
  // object reference across derives until this tab itself changes.
  const ownTab = useTabStore(
    (s) => s.tabs.find((tab) => tab.id === tabId) ?? null
  )
  // Resolve this panel's folder from ITS OWN tab, not the global active folder.
  // A keep-alive panel for a background tab must NOT re-render when the active
  // tab switches to a different folder. For the active tab this equals the old
  // `activeFolderId` (which is itself derived from the active tab's folderId via
  // `syncActiveFolderId`); it also avoids the brief post-switch window where the
  // global `activeFolderId` still lags on the previous tab's folder (same
  // rationale as the per-tab `workingDir` used for the connection below).
  const ownFolderId = ownTab?.folderId ?? null
  const folder = useAppWorkspaceStore((s) =>
    ownFolderId != null
      ? (s.allFolders.find((f) => f.id === ownFolderId) ?? null)
      : null
  )
  const folderId = ownFolderId ?? 0
  const {
    bindConversationTab,
    setChatDraftWorkingDir,
    setTabRuntimeConversationId,
    pinTab,
    openNewConversationTab,
    closeTab,
    consumePendingComposerText,
    confirmDraftAgent,
    setDraftAgentFromFallback,
  } = useTabActions()
  const { setSessionStats } = useSessionStats()
  const {
    appendOptimisticTurn,
    removeOptimisticTurn,
    appendViewerUserTurn,
    completeTurn,
    refetchDetail,
    syncTurnMetadata,
    removeConversation,
    setAcpLoadError,
    setDbConversationId,
    setExternalId,
    setLiveMessage,
    setPendingCleanup,
    setSyncState,
  } = useConversationRuntimeActions()
  const acpActions = useAcpActions()

  // Stable runtime session key — set once at mount, never changes.
  // For new conversations this is a virtual (negative) ID; for existing
  // conversations opened from the sidebar it equals the real DB ID.
  const [effectiveConversationId] = useState(
    () => conversationId ?? buildVirtualConversationId(`draft-${tabId}`)
  )
  const [createdConversationId, setCreatedConversationId] = useState<
    number | null
  >(null)
  const dbConversationId = conversationId ?? createdConversationId
  const { sortedTypes: usableAgentTypes, fresh: agentsLoaded } =
    useSortedAvailableAgents()
  const [draftAgentType, setDraftAgentType] = useState<AgentType>(agentType)
  const selectedAgent =
    conversationId != null
      ? agentType
      : agentsLoaded && !usableAgentTypes.includes(draftAgentType)
        ? (usableAgentTypes[0] ?? draftAgentType)
        : draftAgentType
  useEffect(() => {
    if (accountStatus !== "authenticated") return
    let active = true
    void loadFixedAgentOptions(selectedAgent).then(() => {
      if (active) setCatalogVersion((version) => version + 1)
    })
    return () => {
      active = false
    }
  }, [accountStatus, selectedAgent])
  useEffect(() => {
    if (conversationId !== null || accountStatus !== "authenticated") return
    void refreshFixedAgentOptions(selectedAgent).then(() =>
      setCatalogVersion((version) => version + 1)
    )
  }, [conversationId, accountStatus, selectedAgent])
  // Seed from localStorage so the React state reflects the user's saved
  // mode for this agent immediately on mount. Without this seed, a reuse-
  // path connect (idle window after a refresh, before the agent is GC'd)
  // would silently fall back to whatever `current_mode_id` the backend
  // happens to be on: `handleModeChange` updates only React state and
  // localStorage, not the agent — the agent gets synced inside
  // `handleSend` by diffing `modeId` against `modes.current_mode_id`.
  // A null seed here means that diff is "agent default vs null", which
  // resolves the displayed mode through `conn.modes.current_mode_id`
  // and never triggers the catch-up `setMode`.
  const [modeId, setModeId] = useState<string | null>(() =>
    getSavedModeId(agentType)
  )
  const [draftConfigValues, setDraftConfigValues] = useState<
    Record<string, string>
  >(() => getSavedPrefsForConnect(agentType).configValues ?? {})
  const [modelReapplyAttempt, setModelReapplyAttempt] = useState<{
    target: string
    previousModel: string
    sourceConnectionId: string
  } | null>(null)
  const [requestedModel, setRequestedModel] = useState<string | null>(null)
  useEffect(() => {
    setRequestedModel(null)
    setModelReapplyAttempt(null)
  }, [selectedAgent])
  const [sendSignal, setSendSignal] = useState(0)
  const usableAgentCount = usableAgentTypes.length
  const [agentConnectError, setAgentConnectError] = useState<string | null>(
    null
  )
  const [hasSentMessage, setHasSentMessage] = useState(false)
  const [quickActionInject, setQuickActionInject] =
    useState<ComposerInjectContent | null>(null)
  const [contextPrimerLoading, setContextPrimerLoading] = useState(false)

  const hasPersistedConversation = dbConversationId != null

  // A folderless chat draft before its first send (chat tab, not yet persisted).
  // Used to trigger the eager scratch-dir prepare below, which gives the draft a
  // real workingDir so the ACP connection can start as early as possible. The
  // composer remains usable meanwhile and queues submissions until ready. Once
  // bound it has a persisted row + workingDir and this is false.
  const isChatDraft = useMemo(
    () => ownTab?.isChat === true && !hasPersistedConversation,
    [ownTab, hasPersistedConversation]
  )

  // Expose the runtime session key to the tab so the aux panel (Diff sidebar)
  // can look up live turns even before the DB conversation is created.
  useEffect(() => {
    if (effectiveConversationId !== conversationId) {
      setTabRuntimeConversationId(tabId, effectiveConversationId)
    }
  }, [
    tabId,
    effectiveConversationId,
    conversationId,
    setTabRuntimeConversationId,
  ])

  // Clear pendingCleanup when tab is (re)opened
  useEffect(() => {
    setPendingCleanup(effectiveConversationId, false)
  }, [effectiveConversationId, setPendingCleanup])

  const latestReloadSignal = useRef(reloadSignal)
  const pendingReloadState = useRef<{
    signal: number
    sawLoading: boolean
  } | null>(null)
  const dbConvIdRef = useRef<number | null>(conversationId)
  const mountedRef = useRef(true)
  const selectedAgentRef = useRef(selectedAgent)
  const createConversationPendingRef = useRef(false)
  // Single-flight guard for the eager scratch-dir prepare (on chat-mode select).
  const prepareChatDirPendingRef = useRef(false)
  const sessionIdRef = useRef<string | null>(null)
  const syncCancelRef = useRef<(() => void) | null>(null)
  const messageScrollPositionRef = useRef<MessageScrollPosition | null>(null)

  useEffect(() => {
    dbConvIdRef.current = dbConversationId
    if (
      dbConversationId != null &&
      dbConversationId !== effectiveConversationId
    ) {
      setDbConversationId(effectiveConversationId, dbConversationId)
    }
  }, [dbConversationId, effectiveConversationId, setDbConversationId])

  useEffect(() => {
    selectedAgentRef.current = selectedAgent
  }, [selectedAgent])

  useEffect(() => {
    if (conversationId != null || selectedAgent === draftAgentType) return
    setDraftAgentType(selectedAgent)
    setModeId(getSavedModeId(selectedAgent))
    setDraftConfigValues(
      getSavedPrefsForConnect(selectedAgent).configValues ?? {}
    )
    setDraftAgentFromFallback(tabId, selectedAgent)
  }, [
    conversationId,
    draftAgentType,
    selectedAgent,
    setDraftAgentFromFallback,
    tabId,
  ])

  // Eagerly create the chat-mode scratch dir the moment this becomes an unbound
  // chat draft, so the ACP connection can spawn at a real cwd BEFORE the first
  // send — picking "no-folder mode" no longer leaves the agent unconnected.
  // Filesystem-only (writes no DB rows), so the lazy-conversation invariant
  // holds; the first send reuses this dir via createChatConversation(existingDir),
  // keeping the connection's cwd put across the bind. Single-flight and
  // self-disarming: once workingDir lands the guard flips false. openChatModeTab
  // clears workingDir on re-entry, so a fresh dir is prepared each time.
  useEffect(() => {
    if (!isActive || !isChatDraft || workingDir) return
    if (prepareChatDirPendingRef.current) return
    prepareChatDirPendingRef.current = true
    void (async () => {
      try {
        const res = await createChatDir()
        if (mountedRef.current) {
          setChatDraftWorkingDir(tabId, res.path)
        }
      } catch (e) {
        // The connection needs this scratch dir before queued messages can flush.
        // Surface creation failures on the welcome screen so the user can retry
        // by re-entering chat mode instead of leaving the queue stalled silently.
        console.error("[ConversationTabView] prepare chat dir:", e)
        if (mountedRef.current) {
          setAgentConnectError(tWelcome("prepareSessionFailed"))
        }
      } finally {
        prepareChatDirPendingRef.current = false
      }
    })()
  }, [
    isActive,
    isChatDraft,
    workingDir,
    tabId,
    setChatDraftWorkingDir,
    tWelcome,
  ])

  // Sync the agentType prop into draftAgentType for draft tabs. The prop
  // changes when openNewConversationTab re-points an existing draft at a
  // different folder's default agent (or when any other external mutation
  // updates tab.agentType). Without this mirror, the local draftAgentType
  // would stay frozen at its mount value and the UI/connection would not
  // follow. Persisted conversations read agentType directly from the prop
  // via selectedAgent, so they are unaffected.
  useEffect(() => {
    if (conversationId != null) return
    if (agentType === selectedAgentRef.current) return
    setDraftAgentType(agentType)
    setModeId(getSavedModeId(agentType))
    setDraftConfigValues(getSavedPrefsForConnect(agentType).configValues ?? {})
    setAgentConnectError(null)
  }, [agentType, conversationId])

  const {
    detail,
    loading: detailLoading,
    error: detailError,
    acpLoadError,
  } = useConversationDetail(effectiveConversationId, {
    enabled: isVisible && isDocumentVisible,
  })

  // Subscribe to only the fields this panel actually reads from its runtime
  // session — NOT the whole session object. The live-message sink rewrites the
  // session object on every streaming batch (~60/s, via SET_LIVE_MESSAGE); a
  // whole-object selector here would re-render this keep-alive panel (and the
  // composer subtree it wraps) on every streaming token, even though none of
  // these scalar fields change mid-stream. `useShallow` keeps the returned slice
  // reference-stable across batches, so the panel re-renders only when one of
  // them actually changes. (message-list-view subscribes to the session's
  // liveMessage separately to render the live stream.)
  const {
    sessionStats: effectiveSessionStats,
    externalId: runtimeExternalId,
    syncState: runtimeSyncState,
    hasLiveResponseContent,
  } = useConversationRuntimeStore(
    useShallow((s) => {
      const session = s.byConversationId.get(effectiveConversationId)
      return {
        sessionStats: session?.sessionStats ?? null,
        externalId: session?.externalId ?? null,
        syncState: session?.syncState ?? "idle",
        hasLiveResponseContent: Boolean(session?.liveMessage?.content.length),
      }
    })
  )

  useEffect(() => {
    if (!isActive) return
    setSessionStats(effectiveSessionStats)
  }, [effectiveSessionStats, isActive, setSessionStats])

  // Two-source resolution for the session id passed to acp_connect:
  //   1. detail.summary.external_id — DB value, available for tabs opened
  //      from the sidebar (effectiveConversationId equals the real cid).
  //   2. runtimeExternalId — populated by the connSessionId effect
  //      below when SessionStarted fires. This is the ONLY source for tabs
  //      that started as a new conversation: their effectiveConversationId
  //      is locked to a virtual negative id (line 186 useState initializer
  //      runs once), useConversationDetail skips fetching for virtual ids,
  //      and detail stays null forever. Without this fallback, every
  //      reconnect on a new-conversation tab passes sessionId=undefined →
  //      backend takes session/new → DB.external_id is overwritten on the
  //      next prompt → original sid orphaned, agent loses prior context.
  const externalId =
    detail?.summary.external_id ?? runtimeExternalId ?? undefined
  // For persisted conversations opened from the sidebar, wait until the
  // session's external_id has been resolved before auto-connecting.
  // Otherwise the auto-connect effect fires with sessionId=undefined and
  // the backend falls back to session/new, orphaning the historical
  // context. This applies to every Agent: a non-resumable Agent may still have
  // a live connection to attach to, but must never receive session/new as a
  // silent replacement for historical context.
  const awaitingHistoricalSessionId = hasPersistedConversation && detailLoading
  const canAutoConnect =
    (hasPersistedConversation || (agentsLoaded && usableAgentCount > 0)) &&
    !awaitingHistoricalSessionId &&
    !(hasPersistedConversation && detailError) &&
    !(hasPersistedConversation && acpLoadError)
  const draftStorageKey = useMemo(() => {
    if (dbConversationId != null) {
      return buildConversationDraftStorageKey(dbConversationId)
    }
    return buildNewConversationDraftStorageKey(tabId)
  }, [dbConversationId, tabId])
  useEffect(() => {
    if (dbConversationId == null) {
      adoptLegacyNewConversationDraft(draftStorageKey)
    }
  }, [dbConversationId, draftStorageKey])
  const [hasUnsavedDraft, setHasUnsavedDraft] = useState(false)
  const [hasEphemeralDraft, setHasEphemeralDraft] = useState(false)
  useEffect(
    () =>
      subscribeMessageInputDraftPresence(draftStorageKey, setHasUnsavedDraft),
    [draftStorageKey]
  )
  // Use the per-tab workingDir (derived from the tab's own folderId by the
  // parent) rather than the active folder's path — otherwise switching tabs
  // briefly exposes the previous folder's path to the ACP auto-connect
  // effect, and the connection sticks with the wrong cwd.
  const workingDirForConnection = workingDir ?? folder?.path

  const {
    conn,
    autoConnectError,
    ensureConnected,
    handleFocus,
    handleSend: lifecycleSend,
    handleSetConfigOption,
    handleCancel,
    handleRespondPermission,
  } = useConnectionLifecycle({
    contextKey: tabId,
    agentType: selectedAgent,
    isActive: isActive && isDocumentVisible && canAutoConnect,
    workingDir: workingDirForConnection,
    sessionId: dbConversationId != null ? externalId : undefined,
    // Drives cross-client viewer discovery: when another client is already
    // live on this conversation, attach to its connection instead of spawning.
    conversationId: dbConversationId ?? undefined,
    attachOnlyOnActivate: hasPersistedConversation,
    isTransientUnmount: useCallback(
      () => isReparentUnmount(useTabStore.getState(), tabId, groupId),
      [groupId, tabId]
    ),
    silentReconnect: modelReapplyAttempt !== null,
  })
  const { status: connStatus, sessionId: connSessionId } = conn
  const isSilentModelSwitch = modelReapplyAttempt !== null
  const visibleConnStatus = isSilentModelSwitch ? "connected" : connStatus
  const visibleConnError = isSilentModelSwitch ? null : conn.error
  const pendingBalanceRecoveryRef = useRef<PendingBalanceRecovery | null>(null)
  const [dismissedContinuationGeneration, setDismissedContinuationGeneration] =
    useState<number | null>(null)
  const visibleAutoContinuation =
    conn.isViewer ||
    conn.autoContinuation?.source_generation === dismissedContinuationGeneration
      ? null
      : conn.autoContinuation
  const messageQueue = useMessageQueue()
  const {
    queue: msgQueue,
    enqueue: mqEnqueue,
    enqueueAgentInput: mqEnqueueAgentInput,
    requeueItemFront: mqRequeueItemFront,
    getQueueLength: mqGetQueueLength,
    dequeue: mqDequeue,
    remove: mqRemove,
    reorder: mqReorder,
    updateItem: mqUpdateItem,
    editingItemId: mqEditingItemId,
    startEditing: mqStartEditing,
    cancelEditing: mqCancelEditing,
  } = messageQueue

  const rememberSubmittedDraft = useCallback(
    (draft: PromptDraft, modeId?: string | null) => {
      if (!conn.connectionId || conn.isViewer) return
      pendingBalanceRecoveryRef.current = {
        connectionId: conn.connectionId,
        draft,
        modeId: modeId ?? null,
        requeued: false,
      }
    },
    [conn.connectionId, conn.isViewer]
  )

  const handleBalanceRecoveryEvent = useCallback(
    (envelope: EventEnvelope) => {
      if (envelope.connection_id !== conn.connectionId) return
      const pending = pendingBalanceRecoveryRef.current

      if (envelope.type === "turn_complete") {
        if (pending?.connectionId === envelope.connection_id) {
          pendingBalanceRecoveryRef.current = null
        }
        return
      }
      if (
        envelope.type === "status_changed" &&
        envelope.status === "disconnected"
      ) {
        pendingBalanceRecoveryRef.current = null
        return
      }
      if (
        envelope.type !== "error" ||
        !isInsufficientBalanceError(envelope.message, envelope.code) ||
        !pending ||
        pending.requeued
      ) {
        return
      }

      pending.requeued = true
      mqEnqueue(pending.draft, pending.modeId, { blocked: true })
    },
    [conn.connectionId, mqEnqueue]
  )

  useAcpEvent(handleBalanceRecoveryEvent)
  const msgQueueLength = msgQueue.length
  const msgQueueHeadBlocked = msgQueue[0]?.blocked
  const [outboxFlushPending, setOutboxFlushPending] = useState<string | null>(
    null
  )
  const connStatusRef = useRef(connStatus)
  useEffect(() => {
    connStatusRef.current = connStatus
  }, [connStatus])
  const isViewerRef = useRef(conn.isViewer)
  useEffect(() => {
    isViewerRef.current = conn.isViewer
  }, [conn.isViewer])
  // The backend command channel buffers prompts while Initialize/session setup
  // is still running. Accept that fast path only for the connection created for
  // this exact Agent + cwd; during a switch the old connection can remain visible
  // for a render and must never receive the new draft's prompt.
  const connectionReady = canConnectionAcceptPrompt({
    connectionId: conn.connectionId,
    status: connStatus,
    connectedAgentType: conn.agentType,
    intendedAgentType: selectedAgent,
    connectedWorkingDir: conn.connectedWorkingDir,
    intendedWorkingDir: workingDirForConnection,
  })
  const connectionReadyRef = useRef(connectionReady)
  useEffect(() => {
    connectionReadyRef.current = connectionReady
  }, [connectionReady])
  // Present "connecting" to the composer while connected-but-not-ready. The
  // composer still accepts submissions and the send handler queues them until
  // this tab's working directory matches the live connection.
  const composerConnStatus = isSilentModelSwitch
    ? "connected"
    : connStatus === "connected" && !connectionReady
      ? "connecting"
      : connStatus
  const fixedOptions = useMemo(() => {
    void catalogVersion
    return getFixedAgentOptions(
      selectedAgent,
      draftConfigValues,
      tConfig as unknown as SessionConfigTranslator
    )
  }, [selectedAgent, draftConfigValues, tConfig, catalogVersion])
  const connectionModes = useMemo(
    () => fixedOptions.modes?.available_modes ?? [],
    [fixedOptions.modes]
  )
  const connectionConfigOptions = useMemo(() => {
    if (!conn.selectorsReady) return fixedOptions.config_options
    const liveHasModel = conn.configOptions?.some(isModelConfigOption) === true
    return liveHasModel
      ? fixedOptions.config_options
      : fixedOptions.config_options.filter(
          (option) => !isModelConfigOption(option)
        )
  }, [conn.configOptions, conn.selectorsReady, fixedOptions.config_options])
  const canReconcileModelConfig =
    hasAuthoritativeFixedAgentOptions(selectedAgent)
  useEffect(() => {
    if (!canReconcileModelConfig) return
    const next = reconcileModelConfigValues(fixedOptions, draftConfigValues)
    if (next === draftConfigValues) return
    setDraftConfigValues(next)
    replaceConfigPreferences(selectedAgent, next)
  }, [canReconcileModelConfig, draftConfigValues, fixedOptions, selectedAgent])
  const connectionCommands = useMemo(
    () => conn.availableCommands ?? [],
    [conn.availableCommands]
  )

  useEffect(() => {
    if (!connectionReady || connStatus === "prompting") return
    if (
      reconcileModelConfigValues(fixedOptions, draftConfigValues) !==
      draftConfigValues
    ) {
      return
    }
    const commands = planSessionConfigSync(
      connectionConfigOptions,
      conn.configOptions ?? [],
      draftConfigValues
    )
    for (const { configId, valueId } of commands) {
      void handleSetConfigOption(configId, valueId).catch((error: unknown) => {
        if (configId === "model") setRequestedModel(null)
        const live = conn.configOptions?.find(
          (option) => option.id === configId
        )
        if (!live || live.kind.type !== "select") return
        setDraftConfigValues((current) => {
          if (current[configId] !== valueId) return current
          const next = { ...current, [configId]: live.kind.current_value }
          saveConfigPreference(selectedAgent, configId, live.kind.current_value)
          return next
        })
        console.error("[ConversationTabView] config option rejected", {
          configId,
          valueId,
          error,
        })
      })
    }
  }, [
    conn.configOptions,
    connStatus,
    connectionConfigOptions,
    connectionReady,
    draftConfigValues,
    fixedOptions,
    handleSetConfigOption,
    selectedAgent,
  ])

  // A freshly published model may be present in Fusion and the fixed selector
  // before the running ACP process has re-read its native model projection.
  // When the session is idle, reconnect once with the same session id so the
  // Agent advertises the updated model list. Never interrupt a live turn or a
  // connection owned by another client.
  useEffect(() => {
    const target = requestedModel
    const live = conn.configOptions?.find(isModelConfigOption)
    const fixed = fixedOptions.config_options.find(isModelConfigOption)
    const targetIsKnown = Boolean(
      target &&
      fixed?.kind.type === "select" &&
      fixed.kind.options.some((option) => option.value === target)
    )
    if (
      targetIsKnown &&
      live?.kind.type === "select" &&
      !live.kind.options.some((option) => option.value === target) &&
      (conn.isViewer || conn.isDelegationChild)
    ) {
      setRequestedModel(null)
      setDraftConfigValues((current) => {
        if (current.model !== target) return current
        const next = { ...current, model: live.kind.current_value }
        saveConfigPreference(selectedAgent, "model", live.kind.current_value)
        return next
      })
      toast.error(tConfigStale("modelSwitchFailed"))
      return
    }
    if (
      !target ||
      !fixed ||
      fixed.kind.type !== "select" ||
      !fixed.kind.options.some((option) => option.value === target) ||
      !connectionReady ||
      !conn.selectorsReady ||
      conn.isViewer ||
      conn.isDelegationChild ||
      !conn.connectionId
    ) {
      return
    }
    if (connStatus === "prompting") return
    if (!live || live.kind.type !== "select") return
    if (live.kind.options.some((option) => option.value === target)) {
      if (live.kind.current_value === target) {
        setRequestedModel(null)
        if (modelReapplyAttempt?.target === target) {
          setModelReapplyAttempt(null)
        }
        return
      }
      if (!modelReapplyAttempt) {
        return
      }
      if (modelReapplyAttempt.target === target) {
        setModelReapplyAttempt(null)
      }
      setRequestedModel(null)
      setDraftConfigValues((current) => {
        if (current.model !== target) return current
        const next = { ...current, model: live.kind.current_value }
        saveConfigPreference(selectedAgent, "model", live.kind.current_value)
        return next
      })
      toast.error(tConfigStale("modelSwitchFailed"))
      return
    }
    if (modelReapplyAttempt) return

    const sourceConnectionId = conn.connectionId
    setModelReapplyAttempt({
      target,
      previousModel: live.kind.current_value,
      sourceConnectionId,
    })
    void acpActions
      .reapplyConfig(tabId, true, dbConvIdRef.current ?? undefined)
      .catch((error: unknown) => {
        setModelReapplyAttempt(null)
        setRequestedModel(null)
        setDraftConfigValues((current) => {
          if (current.model !== target) return current
          const next = { ...current, model: live.kind.current_value }
          saveConfigPreference(selectedAgent, "model", live.kind.current_value)
          return next
        })
        toast.error(tConfigStale("modelSwitchFailed"), {
          description: error instanceof Error ? error.message : String(error),
        })
      })
  }, [
    acpActions,
    conn.configOptions,
    conn.connectionId,
    conn.isDelegationChild,
    conn.isViewer,
    conn.selectorsReady,
    connStatus,
    connectionConfigOptions,
    connectionReady,
    fixedOptions.config_options,
    modelReapplyAttempt,
    requestedModel,
    selectedAgent,
    tConfigStale,
    tabId,
  ])

  const modelReapplyTarget = modelReapplyAttempt?.target
  const modelReapplyPreviousModel = modelReapplyAttempt?.previousModel
  useEffect(() => {
    if (!modelReapplyTarget || modelReapplyPreviousModel == null) return
    const target = modelReapplyTarget
    const fallbackModel = modelReapplyPreviousModel
    const timer = setTimeout(() => {
      setModelReapplyAttempt((current) =>
        current?.target === target ? null : current
      )
      setRequestedModel((current) => (current === target ? null : current))
      setDraftConfigValues((current) => {
        if (current.model !== target) return current
        const next = { ...current, model: fallbackModel }
        saveConfigPreference(selectedAgent, "model", fallbackModel)
        return next
      })
      toast.error(tConfigStale("modelSwitchFailed"))
    }, MODEL_REAPPLY_TIMEOUT_MS)
    return () => clearTimeout(timer)
  }, [
    modelReapplyPreviousModel,
    modelReapplyTarget,
    selectedAgent,
    tConfigStale,
  ])

  // Validate the first post-reconnect selector snapshot. If the Agent still
  // does not advertise the requested model, roll back instead of leaving a
  // permanently misleading fixed-catalog selection in the composer.
  useEffect(() => {
    if (
      !modelReapplyAttempt ||
      conn.connectionId === modelReapplyAttempt.sourceConnectionId ||
      !conn.selectorsReady
    ) {
      return
    }
    const live = conn.configOptions?.find(isModelConfigOption)
    if (!live || live.kind.type !== "select") return
    if (
      live.kind.options.some(
        (option) => option.value === modelReapplyAttempt.target
      ) && live.kind.current_value === modelReapplyAttempt.target
    ) {
      setModelReapplyAttempt(null)
      return
    }
    const target = modelReapplyAttempt.target
    const fallbackModel =
      live.kind.current_value || modelReapplyAttempt.previousModel
    setDraftConfigValues((current) => {
      if (current.model !== target) return current
      const next = { ...current, model: fallbackModel }
      saveConfigPreference(selectedAgent, "model", fallbackModel)
      return next
    })
    setModelReapplyAttempt(null)
    setRequestedModel(null)
    toast.error(tConfigStale("modelSwitchFailed"))
  }, [
    conn.configOptions,
    conn.connectionId,
    conn.selectorsReady,
    modelReapplyAttempt,
    selectedAgent,
    tConfigStale,
  ])
  const selectedModeId = useMemo(() => {
    if (connectionModes.length === 0) return null
    if (modeId && connectionModes.some((mode) => mode.id === modeId)) {
      return modeId
    }
    return conn.modes?.current_mode_id ?? connectionModes[0]?.id ?? null
  }, [conn.modes?.current_mode_id, connectionModes, modeId])

  useEffect(() => {
    if (connSessionId) {
      sessionIdRef.current = connSessionId
    }
  }, [connSessionId])

  // Mirror the connection's load failure (set on `session_load_failed` from
  // the agent) onto the per-conversation runtime session so the detail UI
  // can surface it next to detail-load errors. Cleared automatically when
  // the connection's loadError clears (e.g. via Reload).
  const connLoadError = conn.loadError
  useEffect(() => {
    setAcpLoadError(effectiveConversationId, connLoadError ?? null)
  }, [connLoadError, effectiveConversationId, setAcpLoadError])

  // Promote the completed turn on the prompting→idle edge. (There is no longer
  // an ordering constraint against a setLiveMessage cleanup: the liveMessage
  // sink writes the runtime store from the connection dispatch, not a React
  // effect — see registerLiveMessageSink.)
  const prevConnStatusRef = useRef(connStatus)
  useEffect(() => {
    const wasPrompting = prevConnStatusRef.current === "prompting"
    prevConnStatusRef.current = connStatus
    if (!wasPrompting || connStatus === "prompting") return

    // Turn completed — promote liveMessage + optimisticTurns to localTurns.
    // Don't pass conn.liveMessage: this panel no longer subscribes to it (the
    // connection snapshot is stable across streaming tokens — see useConnection),
    // so reading it here would be stale. COMPLETE_TURN falls back to
    // session.liveMessage, which the connection dispatch's sink wrote
    // synchronously as the final chunk landed (turn_complete flushes the stream
    // queue BEFORE the status change), so it already holds the final message.
    completeTurn(effectiveConversationId)

    // Cancel previous metadata sync (handles rapid consecutive turns)
    syncCancelRef.current?.()
    syncCancelRef.current = null

    const persistedId = dbConvIdRef.current
    if (persistedId && persistedId > 0) {
      syncCancelRef.current = syncTurnMetadata(
        persistedId,
        effectiveConversationId
      )
    }
  }, [completeTurn, connStatus, effectiveConversationId, syncTurnMetadata])

  // Auto-send queued messages when agent finishes responding.
  // Refs are synced via useEffect; the auto-send effect is declared
  // AFTER completeTurn so React runs it second.
  const autoSendQueueRef = useRef<() => QueuedMessage | undefined>(mqDequeue)
  useEffect(() => {
    autoSendQueueRef.current = mqDequeue
  }, [mqDequeue])
  const handleSendRef = useRef<
    (
      draft: PromptDraft,
      modeId?: string | null,
      opts?: {
        fromQueueFlush?: boolean
        queuedMessage?: QueuedMessage
        scenarioPrepared?: boolean
      }
    ) => boolean | void | Promise<boolean>
  >(() => {})
  // Timestamp of the last send that bounced with TurnBusyError. The flush below
  // backs off after a bounce so repeated busy rejections (backend still running
  // another turn while this client believes it is idle) don't spin one failed
  // send per round-trip.
  const lastFlushBounceAtRef = useRef(0)

  // Flush queued messages whenever the agent is idle. This is the queue's send
  // engine, covering BOTH:
  //   - the normal case: a message queued while the agent was prompting, sent
  //     once the turn completes (prompting→connected drives syncState→idle); and
  //   - a draft re-queued by a bounced concurrent send that landed AFTER the
  //     prompting→connected transition already passed — which an edge-triggered
  //     flush would strand until the next turn.
  // Gated on syncState !== "awaiting_persist" so exactly one item flushes at a
  // time: dequeuing + sending appends an optimistic turn → awaiting_persist,
  // which blocks re-entry until that send settles (the turn completes, or it
  // bounces and rolls back to idle to retry the next item). A bounce backoff
  // rate-limits retries against a still-busy backend.
  useEffect(() => {
    if (!outboxFlushPending) return
    const item = conn.agentInputs.find(
      (candidate) => candidate.id === outboxFlushPending
    )
    if (
      connStatus === "prompting" ||
      item?.status === "consumed" ||
      item?.status === "failed" ||
      item?.status === "deleted"
    ) {
      setOutboxFlushPending(null)
    }
  }, [conn.agentInputs, connStatus, outboxFlushPending])

  useEffect(() => {
    if (!connectionReady) return
    if (runtimeSyncState === "awaiting_persist") return
    if (outboxFlushPending) return
    if (msgQueueLength === 0) return
    if (msgQueueHeadBlocked) return
    // setTimeout (not microtask) so a COMPLETE_TURN commit settles first AND so
    // a just-bounced retry waits out the backoff window before re-sending.
    const wait = flushRetryDelayMs(Date.now(), lastFlushBounceAtRef.current)
    const timer = setTimeout(() => {
      if (!connectionReadyRef.current) return
      if (!ensureConversationPointsAvailable()) return
      const next = autoSendQueueRef.current()
      if (next) {
        // Mark this as the queue auto-flush: it sends the dequeued head now and,
        // on a bounce, returns it to the FRONT (vs a direct send → tail).
        handleSendRef.current(next.draft, next.modeId, {
          fromQueueFlush: true,
          queuedMessage: next,
        })
      }
    }, wait)
    return () => clearTimeout(timer)
  }, [
    connectionReady,
    runtimeSyncState,
    msgQueueLength,
    msgQueueHeadBlocked,
    outboxFlushPending,
    ensureConversationPointsAvailable,
  ])

  // Mirror the connection's liveMessage into the runtime session OUTSIDE React.
  // The connection dispatch invokes this sink synchronously whenever liveMessage
  // changes (streaming deltas, tool updates, the prompt-start reset), so the
  // streaming content flows straight to the runtime store — which the message
  // list renders — WITHOUT this keep-alive panel re-rendering per token (the old
  // mirror effect required a per-token render just to run). The sink writes
  // non-null values with isLive = (status === "prompting"), which tells the
  // runtime reducer to bypass its stale-reconnect-replay guard (matters for the
  // rekey path: close+reopen mid-turn, where detail.turns may already hold user
  // turns that would otherwise drop the live assistant stream). Turn-end clearing
  // is owned by COMPLETE_TURN (nulls liveMessage); unmount clearing by
  // removeConversation. `tabId` is the connection contextKey.
  useEffect(() => {
    return acpActions.registerLiveMessageSink(
      tabId,
      effectiveConversationId,
      (liveMessage, isLive) =>
        setLiveMessage(effectiveConversationId, liveMessage, isLive)
    )
  }, [acpActions, tabId, effectiveConversationId, setLiveMessage])

  // Cross-client VIEWER (Bug 2): mirror the connection's in-flight user prompt
  // (from a snapshot's `pending_user_message`, captured when we attach
  // mid-turn) into the runtime as a synthesized user turn. The reducer
  // sender-guards + dedups by id, so this is a no-op on the sender and
  // idempotent against the live `user_message` event below. This branch covers
  // the prompt that was sent BEFORE we attached; the live handler covers
  // prompts sent AFTER.
  useEffect(() => {
    const pending = conn.pendingUserMessage
    if (!pending) return
    appendViewerUserTurn(
      effectiveConversationId,
      buildUserTurnFromMessageBlocks(pending.messageId, pending.blocks)
    )
  }, [conn.pendingUserMessage, effectiveConversationId, appendViewerUserTurn])

  // Cross-client VIEWER (Bug 2): a `user_message` event for THIS connection
  // that arrives while we're attached. The owner added its user turn
  // optimistically; a viewer only receives the assistant stream, so without
  // this the reply would render with no user message above it. Sender-guarded +
  // idempotent in the reducer (the sender's own echo is a no-op).
  useAcpEvent(
    useCallback(
      (envelope: EventEnvelope) => {
        if (envelope.type !== "user_message") return
        if (envelope.connection_id !== conn.connectionId) return
        appendViewerUserTurn(
          effectiveConversationId,
          buildUserTurnFromMessageBlocks(envelope.message_id, envelope.blocks)
        )
      },
      [conn.connectionId, effectiveConversationId, appendViewerUserTurn]
    )
  )

  useEffect(() => {
    if (effectiveConversationId <= 0) return
    setExternalId(effectiveConversationId, detail?.summary.external_id ?? null)
  }, [effectiveConversationId, detail?.summary.external_id, setExternalId])

  useEffect(() => {
    if (!connSessionId) return
    setExternalId(effectiveConversationId, connSessionId)
  }, [connSessionId, effectiveConversationId, setExternalId])

  useEffect(() => {
    if (dbConversationId == null) return
    if (reloadSignal === latestReloadSignal.current) return
    latestReloadSignal.current = reloadSignal
    pendingReloadState.current = {
      signal: reloadSignal,
      sawLoading: false,
    }
    refetchDetail(dbConversationId)
  }, [dbConversationId, reloadSignal, refetchDetail])

  useEffect(() => {
    const pending = pendingReloadState.current
    if (!pending) return

    if (detailLoading) {
      pending.sawLoading = true
      return
    }

    if (!pending.sawLoading) return

    pendingReloadState.current = null

    if (detailError) {
      toast.error(t("reloadFailed", { message: detailError }))
      return
    }

    toast.success(t("reloaded"))
  }, [detailLoading, detailError, t])

  // Cleanup runtime data on unmount (tab close)
  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      syncCancelRef.current?.()
      if (connStatusRef.current === "prompting" && !isViewerRef.current) {
        // Owner, agent still responding — keep the session for deferred cleanup
        // (the background turn_complete handler removes it once done).
        setPendingCleanup(effectiveConversationId, true)
      } else {
        // Idle owner, or a VIEWER (any status): remove immediately. A viewer's
        // unmount detaches its attach subscription, so no turn_complete will
        // arrive to resolve a deferred cleanup — deferring would leak the
        // runtime session (especially in web mode, which has no event firehose
        // after detach).
        removeConversation(effectiveConversationId)
      }
    }
  }, [effectiveConversationId, removeConversation, setPendingCleanup])

  const handleSend = useCallback(
    (
      draft: PromptDraft,
      selectedModeIdArg?: string | null,
      // `fromQueueFlush` marks the auto-flush draining the queue head — that
      // path always sends and, on a bounce, re-queues at the FRONT. A direct
      // input send (no flag) must NOT jump ahead of already-queued items: when
      // a queue exists it tail-enqueues instead of sending, and on a bounce it
      // re-queues at the TAIL.
      opts?: {
        fromQueueFlush?: boolean
        queuedMessage?: QueuedMessage
        scenarioPrepared?: boolean
      }
    ) => {
      const fromQueueFlush = opts?.fromQueueFlush ?? false
      if (!ensureConversationPointsAvailable()) {
        if (fromQueueFlush && opts?.queuedMessage) {
          mqRequeueItemFront({ ...opts.queuedMessage, blocked: true })
        }
        return false
      }
      const packageRef = draft.skillPackage
      if (packageRef && !opts?.scenarioPrepared) {
        const prepareAndContinue = async (): Promise<boolean> => {
          const toastId = toast.loading(
            `正在准备技能包 ${packageRef.slug}@${packageRef.version}`
          )
          try {
            await prepareScenarioPackage(
              packageRef,
              selectedAgent,
              workingDirForConnection
            )
            toast.success(`技能包 ${packageRef.slug} 已就绪`, { id: toastId })
            const continued = handleSendRef.current(
              { ...draft, skillPackage: undefined },
              selectedModeIdArg,
              { ...opts, scenarioPrepared: true }
            )
            return continued instanceof Promise
              ? continued
              : continued !== false
          } catch (error) {
            toast.error("技能包准备失败", {
              id: toastId,
              description:
                error instanceof Error ? error.message : String(error),
            })
            return false
          }
        }
        return prepareAndContinue()
      }
      // Capture the tab's chat-draft state + eager scratch dir synchronously.
      // The user may submit before the Agent connects; that branch queues below
      // and the existing flush effect resumes this same handler once ready.
      const sendOwnTab = ownTab

      if (
        shouldBlockUnboundSend(
          hasPersistedConversation,
          agentsLoaded,
          usableAgentCount
        )
      ) {
        setAgentConnectError(tWelcome("enableAgentFirstPlaceholder"))
        return
      }
      if (shouldQueueBeforeConnection(connectionReady, fromQueueFlush)) {
        const conversationId = dbConvIdRef.current
        if (conversationId != null) {
          const messageId = `agent-input-${randomUUID()}`
          mqEnqueueAgentInput(messageId, draft, selectedModeIdArg ?? null)
          void queueAgentInput(conversationId, messageId, {
            blocks: draft.blocks,
            display_text: draft.displayText,
            mode_id: selectedModeIdArg ?? null,
          })
            .then(() => {
              mqRemove(messageId)
            })
            .catch((error) => {
              console.error("[agent-input] durable recovery queue failed", {
                conversationId,
                messageId,
                error,
              })
              saveMessageInputDraft(
                buildConversationDraftStorageKey(conversationId),
                draft.displayText
              )
              toast.error(tAgentInput("durableQueueFailed"))
            })
        } else {
          mqEnqueue(draft, selectedModeIdArg ?? null)
        }
        setHasSentMessage(true)
        void ensureConnected().catch((error) => {
          console.error("[ConversationTabView] restore before send:", error)
        })
        return
      }
      if (!connectionReady) return
      // Preserve FIFO: a direct send issued while the queue is non-empty joins
      // the tail rather than racing ahead of the queued items. Read the
      // queue length synchronously (it reflects a same-tick bounce requeue).
      if (shouldQueueDirectSend(fromQueueFlush, mqGetQueueLength())) {
        mqEnqueue(draft, selectedModeIdArg ?? null)
        return
      }

      const queuedMessage = opts?.queuedMessage
      if (fromQueueFlush && queuedMessage?.delivery === "agent_input") {
        const connectionId = conn.connectionId
        const conversationId = dbConvIdRef.current
        if (!connectionId || conversationId == null) {
          mqRequeueItemFront(queuedMessage)
          return
        }
        setOutboxFlushPending(queuedMessage.id)
        rememberSubmittedDraft(queuedMessage.draft, queuedMessage.modeId)
        void submitAgentInput(connectionId, conversationId, queuedMessage.id, {
          blocks: queuedMessage.draft.blocks,
          display_text: queuedMessage.draft.displayText,
          mode_id: queuedMessage.modeId,
        })
          .then((item) => {
            if (
              item.status === "consumed" ||
              item.status === "failed" ||
              item.status === "deleted"
            ) {
              setOutboxFlushPending(null)
            }
          })
          .catch((error) => {
            console.error("[agent-input] queued submission failed", {
              messageId: queuedMessage.id,
              error,
            })
            setOutboxFlushPending(null)
            mqRequeueItemFront(queuedMessage)
          })
        return
      }

      // Single-flight the unbound new-tab create. A second direct submit fired
      // before the first create resolves (a double Enter / double click) would
      // otherwise append an optimistic turn it can never deliver: the
      // createConversationPendingRef guard further down returns AFTER the
      // optimistic append. Reject the duplicate here, before any optimistic
      // mutation. Only the unbound path (no persisted id yet) is single-flighted,
      // so persisted sends keep their concurrent queued-send behavior. Applies
      // equally to chat and normal new conversations.
      if (
        shouldRejectDuplicateCreate(
          dbConvIdRef.current != null,
          createConversationPendingRef.current
        )
      ) {
        return
      }

      const optimisticTurn = buildOptimisticUserTurnFromDraft(
        draft,
        sharedT("attachedResources"),
        queuedMessage ? `optimistic-${queuedMessage.id}` : undefined
      )
      const needsImageFallback =
        !conn.promptCapabilities.image &&
        draft.blocks.some((block) => block.type === "image")
      appendOptimisticTurn(
        effectiveConversationId,
        optimisticTurn,
        optimisticTurn.id
      )
      setSendSignal((prev) => prev + 1)
      setSyncState(effectiveConversationId, "awaiting_persist")
      setHasSentMessage(true)

      // Backend rejected the send because a turn was already in flight (another
      // co-controlling client, or a "prompting" status this client hadn't
      // observed yet). Roll back the optimistic user turn and drop the draft
      // into the queue above the input box — it auto-sends when the current
      // turn completes, identical to enqueuing while already prompting. Stamp
      // the bounce so the flush backs off instead of immediately retrying.
      let turnBounced = false
      const onTurnInProgress = () => {
        turnBounced = true
        lastFlushBounceAtRef.current = Date.now()
        removeOptimisticTurn(effectiveConversationId, optimisticTurn.id)
        // FIFO: the auto-flush draft WAS the queue head → return it to the
        // front; a direct send (queue was empty when it left) → tail.
        if (fromQueueFlush) {
          if (queuedMessage) {
            mqRequeueItemFront(queuedMessage)
          } else {
            mqEnqueue(draft, selectedModeIdArg ?? null)
          }
        } else {
          mqEnqueue(draft, selectedModeIdArg ?? null)
        }
      }
      const onImageAnalysisError = (error: unknown) => {
        const message = error instanceof Error ? error.message : String(error)
        toast.error(t("imageAnalysisFailed", { message }))
      }
      const onImageAnalysisRejected = (accepted: boolean) => {
        if (accepted || turnBounced) return true
        removeOptimisticTurn(effectiveConversationId, optimisticTurn.id)
        setSyncState(effectiveConversationId, "idle")
        if (fromQueueFlush && queuedMessage) {
          mqRequeueItemFront({ ...queuedMessage, blocked: true })
        }
        return false
      }

      // Pin the tab if it was a temporary preview (single-click opened)
      if (ownTab && !ownTab.isPinned) {
        pinTab(tabId)
      }

      const persistedId = dbConvIdRef.current
      if (persistedId) {
        // Existing-tab path: row already exists, send immediately with the
        // conversation_id pinned so the backend reuses our row instead of
        // creating a duplicate.
        rememberSubmittedDraft(draft, selectedModeIdArg)
        const sending = lifecycleSend(draft, selectedModeIdArg, {
          folderId,
          conversationId: persistedId,
          // The backend echoes this as the broadcast UserMessage's message_id,
          // so viewers' synthesized user turn dedups against our own optimistic
          // turn by exact id (and never suppresses a different sender's prompt).
          clientMessageId: optimisticTurn.id,
          onTurnInProgress,
          onError: needsImageFallback ? onImageAnalysisError : undefined,
        })
        if (needsImageFallback) {
          return sending.then(onImageAnalysisRejected)
        }
        return sending.then((accepted) => accepted || turnBounced)
      }

      // New-tab path: create the DB row first, then send with the new id
      // pinned. This prevents the backend's send_prompt_linked from racing
      // us to create its own conversation row. A folderless chat draft creates
      // via createChatConversation (reusing the eager scratch dir) and binds to
      // its hidden chat folder; every other step — the optimistic turn
      // appended above, the inline lifecycleSend, the rollback — is identical to
      // a normal new conversation. This is the whole point of the fix: after the
      // scratch dir exists, chat mode shares the normal send path and never
      // depends on the flush-on-connect queue to deliver its first prompt.
      if (createConversationPendingRef.current) {
        return needsImageFallback ? Promise.resolve(false) : undefined
      }
      createConversationPendingRef.current = true
      const title = getPromptDraftDisplayText(
        draft,
        sharedT("attachedResources")
      ).slice(0, 80)
      const chatSend = sendOwnTab?.isChat === true
      const chatExistingDir = sendOwnTab?.workingDir

      const createAndSend = async (): Promise<boolean> => {
        try {
          let newConversationId: number
          // The send's folderId defaults to the active folder; a chat send
          // overrides it with the backend-created hidden chat folder.
          let sendFolderId = folderId
          if (chatSend) {
            const res = await createChatConversation(
              selectedAgent,
              title,
              chatExistingDir
            )
            newConversationId = res.conversationId
            sendFolderId = res.folderId
            dbConvIdRef.current = newConversationId
            setExternalId(effectiveConversationId, sessionIdRef.current ?? null)
            setDbConversationId(effectiveConversationId, newConversationId)
            if (!mountedRef.current) {
              setPendingCleanup(effectiveConversationId, true)
              refreshConversations()
              return false
            }
            // Seed allFolders with the hidden chat folder so the tab's new
            // folderId resolves (cwd / active-folder) on the next render. bind
            // reuses the eager scratch dir as workingDir, so the connection's
            // cwd does not move and no reconnect is triggered.
            upsertFolder(res.folder)
            setCreatedConversationId(newConversationId)
            bindConversationTab(
              tabId,
              newConversationId,
              selectedAgent,
              title,
              effectiveConversationId,
              res.folderId,
              res.folder.path
            )
          } else {
            newConversationId = await createConversation(
              folderId,
              selectedAgent,
              title
            )
            dbConvIdRef.current = newConversationId
            // Set external ID on the stable virtual session (no migration needed —
            // effectiveConversationId never changes, so the session stays in place).
            // DB persistence of external_id is now backend-driven from
            // send_prompt_linked once the row is linked, so no explicit DB write here.
            setExternalId(effectiveConversationId, sessionIdRef.current ?? null)
            setDbConversationId(effectiveConversationId, newConversationId)
            if (!mountedRef.current) {
              // Component unmounted while creating — mark for deferred cleanup
              // so the background turn_complete handler can clean up later.
              setPendingCleanup(effectiveConversationId, true)
              refreshConversations()
              return false
            }
            setCreatedConversationId(newConversationId)
            bindConversationTab(
              tabId,
              newConversationId,
              selectedAgent,
              title,
              effectiveConversationId
            )
          }
          clearMessageInputDraft(draftStorageKey)
          refreshConversations()

          // Now that the row exists, kick off the actual prompt with the
          // conversation_id pinned so the backend adopts our row instead of
          // creating a duplicate one.
          rememberSubmittedDraft(draft, selectedModeIdArg)
          const accepted = await lifecycleSend(draft, selectedModeIdArg, {
            folderId: sendFolderId,
            conversationId: newConversationId,
            clientMessageId: optimisticTurn.id,
            onTurnInProgress,
            onError: needsImageFallback ? onImageAnalysisError : undefined,
          })
          return needsImageFallback
            ? onImageAnalysisRejected(accepted)
            : accepted || turnBounced
        } catch (e) {
          console.error("[ConversationTabView] create conversation:", e)
          // A failed create (chat OR normal) must fully restore the pre-send
          // state, not strand the user behind a blank panel:
          //   1. drop the optimistic turn (no ghost stuck in awaiting_persist),
          //   2. return syncState to idle,
          //   3. setHasSentMessage(false) → re-enters welcome mode (otherwise the
          //      welcome screen never returns and the list is empty),
          //   4. re-seed the draft text — message-input clears it synchronously on
          //      send, so without this the user's prompt is lost on failure,
          //   5. surface the error on the welcome banner so it isn't silent.
          removeOptimisticTurn(effectiveConversationId, optimisticTurn.id)
          setSyncState(effectiveConversationId, "idle")
          setHasSentMessage(false)
          const draftText = draft.displayText.trim()
          if (draftText) {
            saveMessageInputDraft(draftStorageKey, draftText)
          }
          if (mountedRef.current) {
            setAgentConnectError(tWelcome("createConversationFailed"))
          }
          return false
        } finally {
          createConversationPendingRef.current = false
        }
      }
      const pending = createAndSend()
      return pending
    },
    [
      appendOptimisticTurn,
      removeOptimisticTurn,
      mqEnqueue,
      mqEnqueueAgentInput,
      mqRemove,
      mqRequeueItemFront,
      mqGetQueueLength,
      bindConversationTab,
      agentsLoaded,
      connectionReady,
      effectiveConversationId,
      ensureConnected,
      draftStorageKey,
      folderId,
      hasPersistedConversation,
      lifecycleSend,
      rememberSubmittedDraft,
      pinTab,
      refreshConversations,
      selectedAgent,
      setDbConversationId,
      setExternalId,
      setPendingCleanup,
      setSyncState,
      sharedT,
      ownTab,
      tWelcome,
      tAgentInput,
      tabId,
      upsertFolder,
      usableAgentCount,
      conn.connectionId,
      conn.promptCapabilities.image,
      ensureConversationPointsAvailable,
      workingDirForConnection,
      t,
    ]
  )

  const handleAutoContinuationContinue = useCallback(async () => {
    const continuation = visibleAutoContinuation
    if (!continuation || continuation.phase !== "needs_user_action") return
    const text = tAutoContinuation("continuePrompt")
    const accepted = await handleSend(
      {
        blocks: [{ type: "text", text }],
        displayText: text,
      },
      selectedModeId
    )
    if (accepted !== false) {
      setDismissedContinuationGeneration(continuation.source_generation)
    }
  }, [handleSend, selectedModeId, tAutoContinuation, visibleAutoContinuation])

  const handleAutoContinuationStop = useCallback(() => {
    if (!visibleAutoContinuation) return
    setDismissedContinuationGeneration(
      visibleAutoContinuation.source_generation
    )
  }, [visibleAutoContinuation])

  const enqueueAgentDraft = useCallback(
    (prepared: PromptDraft, selectedModeIdArg: string | null) => {
      const messageId = `agent-input-${randomUUID()}`
      const conversationId = dbConvIdRef.current
      mqEnqueueAgentInput(messageId, prepared, selectedModeIdArg)
      if (conversationId == null) return true
      void queueAgentInput(conversationId, messageId, {
        blocks: prepared.blocks,
        display_text: prepared.displayText,
        mode_id: selectedModeIdArg,
      })
        .then(() => mqRemove(messageId))
        .catch((error) => {
          console.error("[agent-input] durable submission failed", {
            messageId,
            error,
          })
          saveMessageInputDraft(
            buildConversationDraftStorageKey(conversationId),
            prepared.displayText
          )
          toast.error(tAgentInput("durableQueueFailed"))
        })
      return true
    },
    [mqEnqueueAgentInput, mqRemove, tAgentInput]
  )

  const handlePromptingSubmit = useCallback(
    (draft: PromptDraft, selectedModeIdArg: string | null) => {
      if (!ensureConversationPointsAvailable()) return false
      const packageRef = draft.skillPackage
      if (!packageRef) return enqueueAgentDraft(draft, selectedModeIdArg)
      const toastId = toast.loading(
        `正在准备技能包 ${packageRef.slug}@${packageRef.version}`
      )
      return prepareScenarioPackage(
        packageRef,
        selectedAgent,
        workingDirForConnection
      )
        .then(() => {
          toast.success(`技能包 ${packageRef.slug} 已就绪`, { id: toastId })
          return enqueueAgentDraft(
            { ...draft, skillPackage: undefined },
            selectedModeIdArg
          )
        })
        .catch((error) => {
          toast.error("技能包准备失败", {
            id: toastId,
            description: error instanceof Error ? error.message : String(error),
          })
          return false
        })
    },
    [
      ensureConversationPointsAvailable,
      enqueueAgentDraft,
      selectedAgent,
      workingDirForConnection,
    ]
  )

  const handleDeleteAgentInput = useCallback(
    (messageId: string) => {
      const connectionId = conn.connectionId
      const conversationId = dbConvIdRef.current
      if (!connectionId || conversationId == null) return
      void deleteAgentInput(connectionId, conversationId, messageId).catch(
        (error) => {
          console.error("[agent-input] delete failed", { messageId, error })
          toast.error(tAgentInput("deleteFailed"))
        }
      )
    },
    [conn.connectionId, tAgentInput]
  )

  const handleRetryAgentInput = useCallback(
    (messageId: string) => {
      const connectionId = conn.connectionId
      const conversationId = dbConvIdRef.current
      if (!connectionId || conversationId == null) return
      if (!ensureConversationPointsAvailable()) return
      void retryAgentInput(connectionId, conversationId, messageId).catch(
        (error) => {
          console.error("[agent-input] retry failed", { messageId, error })
          toast.error(tAgentInput("retryFailed"))
        }
      )
    },
    [conn.connectionId, ensureConversationPointsAvailable, tAgentInput]
  )

  const handleQueueRetry = useCallback(
    (messageId: string) => {
      if (!ensureConversationPointsAvailable()) return
      const item = msgQueue.find((candidate) => candidate.id === messageId)
      if (!item?.blocked) return
      mqUpdateItem(messageId, item.draft)
    },
    [ensureConversationPointsAvailable, mqUpdateItem, msgQueue]
  )

  const handleReorderAgentInputs = useCallback(
    async (orderedIds: string[]) => {
      const connectionId = conn.connectionId
      const conversationId = dbConvIdRef.current
      if (!connectionId || conversationId == null) return
      try {
        await reorderAgentInputs(connectionId, conversationId, orderedIds)
      } catch (error) {
        console.error("[agent-input] reorder failed", { orderedIds, error })
        toast.error(tAgentInput("reorderFailed"))
        throw error
      }
    },
    [conn.connectionId, tAgentInput]
  )

  const handleForceAgentInputsThrough = useCallback(
    (messageId: string, expectedPrefixIds: string[]) => {
      const connectionId = conn.connectionId
      const conversationId = dbConvIdRef.current
      if (!connectionId || conversationId == null) return
      if (!ensureConversationPointsAvailable()) return
      void forceAgentInputsThrough(
        connectionId,
        conversationId,
        messageId,
        expectedPrefixIds
      ).catch((error) => {
        console.error("[agent-input] safe force failed", {
          messageId,
          expectedPrefixIds,
          error,
        })
        toast.error(tAgentInput("safeForceFailed"))
      })
    },
    [conn.connectionId, ensureConversationPointsAvailable, tAgentInput]
  )

  // Sync handleSend ref for auto-send effect (declared before handleSend)
  useEffect(() => {
    handleSendRef.current = handleSend
  }, [handleSend])

  const executeForkSend = useCallback(
    // Fire-and-forget: the input clears the draft synchronously on click (like a
    // normal send), so there is no in-flight editable window. If the fork can't
    // run right now — disconnected, or the queue is non-empty (a fork is an
    // immediate session side effect and must not jump ahead of queued items) —
    // the draft is NOT lost: it is queued as a normal send (it flushes after any
    // queued items). The same on a fork failure.
    async (draft: PromptDraft, selectedModeIdArg?: string | null) => {
      const connectionId = conn.connectionId
      if (
        !connectionId ||
        connStatus !== "connected" ||
        // Read the queue length SYNCHRONOUSLY so a draft re-queued by a same-
        // tick bounce is seen even before React commits. The UI also hides the
        // fork affordance while the queue is non-empty; this is the guard.
        forkSendBlockedByQueue(mqGetQueueLength())
      ) {
        mqEnqueue(draft, selectedModeIdArg ?? null)
        return
      }
      try {
        // Backend performs all DB writes in one transaction-shaped call:
        // - current row: external_id=S2, title="[Fork] ..."
        // - sibling row: created with external_id=S1, status=pending_review
        const { forkedSessionId } = await acpFork(
          connectionId,
          dbConvIdRef.current,
          folderId
        )
        // Update runtime session id to S2 (frontend in-memory state only)
        sessionIdRef.current = forkedSessionId
        setExternalId(effectiveConversationId, forkedSessionId)

        refreshConversations()
        // Send the message on the forked session (S2)
        handleSend(draft, selectedModeIdArg)
      } catch (err) {
        // Busy (a turn is in flight, e.g. another co-controlling client started
        // one): NOT a fork failure — silently re-queue, like a normal bounce.
        // It sends after the current turn.
        if (err instanceof TurnBusyError) {
          mqEnqueue(draft, selectedModeIdArg ?? null)
          return
        }
        // Real fork failure: surface it. EXPLICIT product decision — fork-send
        // is best-effort, so the draft is never lost; it is re-queued and sent
        // on the current (un-forked) session.
        toast.error(
          t("forkSessionFailed", {
            error:
              err instanceof Error
                ? err.message
                : typeof err === "object" && err !== null
                  ? JSON.stringify(err)
                  : String(err),
          })
        )
        mqEnqueue(draft, selectedModeIdArg ?? null)
      }
    },
    [
      conn.connectionId,
      connStatus,
      mqGetQueueLength,
      mqEnqueue,
      effectiveConversationId,
      folderId,
      handleSend,
      refreshConversations,
      setExternalId,
      t,
    ]
  )

  const handleForkSend = useCallback(
    (draft: PromptDraft, selectedModeIdArg?: string | null) => {
      if (!ensureConversationPointsAvailable()) return false
      void executeForkSend(draft, selectedModeIdArg)
      return true
    },
    [ensureConversationPointsAvailable, executeForkSend]
  )

  const handleOpenAgentsSettings = useCallback(() => {
    openSettingsWindow("agents", { agentType: selectedAgent }).catch((err) => {
      console.error(
        "[ConversationTabView] failed to open settings window:",
        err
      )
    })
  }, [selectedAgent])

  // Manual agent switch only updates local draft state. The single source of
  // truth for (dis)connecting is `useConnectionLifecycle`'s auto-connect
  // effect: when `selectedAgent` changes, the hook re-fires `connect()`,
  // which internally disconnects the old agent's connection at the same
  // contextKey before creating the new one (acp-connections-context.tsx).
  // Doing the disconnect+reconnect here too would race the lifecycle path:
  // a late-returning disconnect would dispatch CONNECTION_REMOVED by
  // contextKey and wipe the new connection's frontend state, leaving a
  // backend orphan.
  const handleAgentSelect = useCallback(
    (nextAgentType: AgentType) => {
      if (nextAgentType === selectedAgentRef.current) return
      if (dbConvIdRef.current) return

      setDraftAgentType(nextAgentType)
      setModeId(getSavedModeId(nextAgentType))
      setDraftConfigValues(
        getSavedPrefsForConnect(nextAgentType).configValues ?? {}
      )
      setAgentConnectError(null)
      // Real user click — clear the provisional flag so TabProvider's
      // correction effect leaves this tab alone.
      confirmDraftAgent(tabId, nextAgentType)
    },
    [confirmDraftAgent, tabId]
  )

  // AgentSelector auto-fallback: the requested default agent was missing
  // or unavailable, so it picked a substitute on its own. Sync local UI
  // state (so the connection points at the right agent immediately) but
  // mark the tab as still provisional — TabProvider's correction effect
  // will re-resolve against the folder's saved default once all three
  // hydration gates are open, and overwrite this substitute if needed.
  const handleAgentFallback = useCallback(
    (nextAgentType: AgentType) => {
      if (nextAgentType === selectedAgentRef.current) return
      if (dbConvIdRef.current) return

      setDraftAgentType(nextAgentType)
      setModeId(getSavedModeId(nextAgentType))
      setDraftConfigValues(
        getSavedPrefsForConnect(nextAgentType).configValues ?? {}
      )
      setAgentConnectError(null)
      setDraftAgentFromFallback(tabId, nextAgentType)
    },
    [setDraftAgentFromFallback, tabId]
  )

  const handleModeChange = useCallback(
    (newModeId: string) => {
      setModeId(newModeId)
      // Persist mode selection to localStorage immediately
      const modes = conn.modes ?? fixedOptions.modes
      if (modes) {
        saveModePreference(selectedAgent, {
          ...modes,
          current_mode_id: newModeId,
        })
      }
    },
    [conn.modes, fixedOptions.modes, selectedAgent]
  )

  const handleConfigOptionChange = useCallback(
    (configId: string, valueId: string) => {
      if (configId === "model") setRequestedModel(valueId)
      setDraftConfigValues((current) => ({
        ...current,
        [configId]: valueId,
      }))
      saveConfigPreference(selectedAgent, configId, valueId)
    },
    [selectedAgent]
  )

  const handleAnswerQuestion = useCallback(
    (answer: string) => {
      if (connStatus !== "connected") return
      if (!ensureConversationPointsAvailable()) return
      const optimisticTurn: MessageTurn = {
        id: `optimistic-${randomUUID()}`,
        role: "user",
        blocks: [{ type: "text", text: answer }],
        timestamp: new Date().toISOString(),
      }
      const draft: PromptDraft = {
        blocks: [{ type: "text", text: answer }],
        displayText: answer,
      }
      appendOptimisticTurn(
        effectiveConversationId,
        optimisticTurn,
        optimisticTurn.id
      )
      setSendSignal((prev) => prev + 1)
      setSyncState(effectiveConversationId, "awaiting_persist")
      rememberSubmittedDraft(draft)
      lifecycleSend(draft, null, {
        clientMessageId: optimisticTurn.id,
        // Rejected because a turn was already in flight — roll back the
        // optimistic turn and re-queue so it isn't stranded or lost.
        onTurnInProgress: () => {
          lastFlushBounceAtRef.current = Date.now()
          removeOptimisticTurn(effectiveConversationId, optimisticTurn.id)
          // A direct answer (never dequeued from the queue) re-queues at the
          // TAIL — it was sent after any already-queued items, so FIFO keeps it
          // behind them. (Only the auto-flush path, whose draft WAS the head,
          // re-queues at the front.)
          mqEnqueue(draft, null)
        },
      })
    },
    [
      appendOptimisticTurn,
      removeOptimisticTurn,
      mqEnqueue,
      connStatus,
      effectiveConversationId,
      ensureConversationPointsAvailable,
      lifecycleSend,
      rememberSubmittedDraft,
      setSyncState,
    ]
  )

  // Answer a blocking multiple-choice `ask_user_question`. Routes straight to
  // the dedicated answer endpoint (NOT a prompt) so it resolves the parked tool
  // call; the backend broadcasts `question_resolved` to clear the card on every
  // client.
  const handleAnswerAskQuestion = useCallback(
    (questionId: string, answer: QuestionAnswer) =>
      acpActions.answerQuestion(tabId, questionId, answer),
    [acpActions, tabId]
  )

  const handleRespondChannelConfirmation = useCallback(
    (confirmationId: string, confirmed: boolean) =>
      acpActions.respondChannelConfirmation(tabId, confirmationId, confirmed),
    [acpActions, tabId]
  )

  // Queue edit flow: derive editing draft text from queue state
  const editingQueueDraftText = useMemo(() => {
    if (!mqEditingItemId) return null
    const item = msgQueue.find((m) => m.id === mqEditingItemId)
    return item?.draft.displayText ?? null
  }, [mqEditingItemId, msgQueue])

  // The editing item's full blocks, so the composer can restore inline badges +
  // attachments (not just the display text) when re-opening a queued message.
  const editingQueueDraftBlocks = useMemo(() => {
    if (!mqEditingItemId) return null
    const item = msgQueue.find((m) => m.id === mqEditingItemId)
    return item?.draft.blocks ?? null
  }, [mqEditingItemId, msgQueue])

  const handleQueueEdit = useCallback(
    (id: string) => {
      mqStartEditing(id)
    },
    [mqStartEditing]
  )

  const handleQueueCancelEdit = useCallback(() => {
    mqCancelEditing()
  }, [mqCancelEditing])

  const handleSaveQueueEdit = useCallback(
    (draft: PromptDraft) => {
      if (mqEditingItemId) {
        mqUpdateItem(mqEditingItemId, draft)
      }
    },
    [mqEditingItemId, mqUpdateItem]
  )

  const showDraftHeader = !hasPersistedConversation && !hasSentMessage
  const isWelcomeMode = showDraftHeader

  const handleQuickAction = useCallback((payload: ComposerInjectContent) => {
    setQuickActionInject(payload)
  }, [])

  useEffect(() => {
    if (hasPersistedConversation || !ownTab?.pendingComposerText) return
    setQuickActionInject({ text: ownTab.pendingComposerText })
  }, [hasPersistedConversation, ownTab?.pendingComposerText])

  const handleQuickActionConsumed = useCallback(() => {
    setQuickActionInject(null)
    consumePendingComposerText(tabId)
  }, [consumePendingComposerText, tabId])

  const canShowDetailErrorActions =
    hasPersistedConversation && dbConversationId != null && !!folder
  const handleReloadDetail = useCallback(() => {
    if (dbConversationId == null) return
    // Clear the ACP load failure so canAutoConnect re-enables and the next
    // auto-connect attempt is allowed to retry session/load. The mirror
    // effect above syncs this back into the runtime session as null.
    if (acpLoadError) {
      acpActions.clearAcpLoadError(tabId)
    }
    refetchDetail(dbConversationId)
  }, [acpActions, acpLoadError, dbConversationId, refetchDetail, tabId])
  // Open (or re-activate) the singleton draft tab BEFORE closing the failing
  // tab. closeTab auto-creates a replacement draft when it removes the last
  // tab, and `openNewConversationTab` reads `rawTabsRef.current` which
  // wouldn't yet reflect either pending update if we closed first — the
  // singleton check would miss the replacement and we'd end up with two
  // drafts. Doing it in this order means the second `setTabs` (closeTab)
  // runs against the result of the first.
  const handleOpenNewSession = useCallback(() => {
    if (!folder) return
    // Retry-from-error: user wants a fresh draft in the same conversation
    // context, so inherit the active tab's agent when the folder has no
    // pinned default.
    openNewConversationTab(folder.id, workingDirForConnection ?? folder.path, {
      inheritFromActive: true,
    })
    closeTab(tabId)
  }, [closeTab, folder, openNewConversationTab, tabId, workingDirForConnection])

  const detailTurns = detail?.turns
  const handleSessionFailureAction = useCallback(
    (action: SessionFailureAction) => {
      switch (action) {
        case "retry": {
          const text =
            lastUserPromptText(
              getTimelineTurns(effectiveConversationId).map(({ turn }) => turn)
            ) ?? lastUserPromptText(detailTurns)
          if (!text) {
            toast.warning(tSessionFailure("retryUnavailable"))
            return
          }
          mqEnqueue(
            { blocks: [{ type: "text", text }], displayText: text },
            selectedModeId
          )
          break
        }
        case "login":
          handleOpenAgentsSettings()
          break
        case "new_session":
          handleOpenNewSession()
          break
      }
    },
    [
      detailTurns,
      effectiveConversationId,
      handleOpenAgentsSettings,
      handleOpenNewSession,
      mqEnqueue,
      selectedModeId,
      tSessionFailure,
    ]
  )

  const handleSessionFailureDismiss = useCallback(
    (ids: string[]) => acpActions.dismissSessionFailures(tabId, ids),
    [acpActions, tabId]
  )

  const handleContinueWithContext = useCallback(async () => {
    if (dbConversationId == null || !folder || contextPrimerLoading) return
    setContextPrimerLoading(true)
    try {
      const primer = await getConversationContextPrimer(dbConversationId)
      setContextPrimerLoading(false)
      openNewConversationTab(
        folder.id,
        workingDirForConnection ?? folder.path,
        {
          inheritFromActive: true,
          folderDefaultAgent: selectedAgent,
          initialComposerText: primer.text,
        }
      )
      closeTab(tabId)
    } catch (error) {
      console.error("[ConversationDetailPanel] build context primer:", error)
      toast.error(t("continueContextFailed"))
      setContextPrimerLoading(false)
    }
  }, [
    closeTab,
    contextPrimerLoading,
    dbConversationId,
    folder,
    openNewConversationTab,
    selectedAgent,
    t,
    tabId,
    workingDirForConnection,
  ])

  const messageListNode = (
    <MessageListView
      conversationId={effectiveConversationId}
      artifactConversationId={dbConversationId}
      agentType={selectedAgent}
      modelName={currentModelName(connectionConfigOptions)}
      modelOptions={connectionConfigOptions}
      connStatus={connStatus}
      isActive={isActive}
      sendSignal={sendSignal}
      sessionStats={effectiveSessionStats}
      detailLoading={detailLoading}
      detailError={detailError}
      acpLoadError={acpLoadError}
      hideEmptyState={!hasPersistedConversation || hasSentMessage}
      onReload={canShowDetailErrorActions ? handleReloadDetail : undefined}
      onNewSession={
        canShowDetailErrorActions ? handleOpenNewSession : undefined
      }
      onContinueWithContext={
        canShowDetailErrorActions ? handleContinueWithContext : undefined
      }
      continueWithContextLoading={contextPrimerLoading}
      onCancel={handleCancel}
      isAwaitingUserInput={Boolean(
        conn.pendingPermission ||
        conn.pendingQuestion ||
        conn.pendingAskQuestion ||
        conn.pendingChannelConfirmation
      )}
      liveTrailingStatus={<BackgroundTasksChip contextKey={tabId} inline />}
      standaloneStatus={<BackgroundTasksChip contextKey={tabId} />}
      scrollPositionRef={messageScrollPositionRef}
    />
  )

  // Live-feedback bar gating + the "agent never read your note" resend fallback.
  // Enqueue rather than `handleSend`: this fallback fires on a turn-end race
  // where the backend already reports no active turn but the frontend may still
  // read `connStatus === "prompting"`, and `handleSend` no-ops unless
  // "connected" — which would silently drop the note. The message queue holds it
  // (visible above the composer) and auto-flushes when the turn completes, so
  // the user's note is never lost.
  const feedbackEnabled = useFeedbackEnabled()
  const resendFeedbackAsPrompt = useCallback(
    (text: string) => {
      mqEnqueue(
        { blocks: [{ type: "text", text }], displayText: text },
        selectedModeId
      )
    },
    [mqEnqueue, selectedModeId]
  )
  const feedback = useSessionFeedback({
    connectionId: conn.connectionId,
    connStatus,
    enabled: feedbackEnabled,
    onResendAsPrompt: resendFeedbackAsPrompt,
  })

  useEffect(() => {
    const protectedState =
      hasUnsavedDraft || msgQueue.length > 0 || mqEditingItemId != null
    acpActions.setPendingInputProtection(tabId, protectedState)
    return () => acpActions.setPendingInputProtection(tabId, false)
  }, [acpActions, hasUnsavedDraft, mqEditingItemId, msgQueue.length, tabId])

  const retainHiddenInput = hasEphemeralDraft || mqEditingItemId != null
  if (!isVisible && !retainHiddenInput) {
    return <div className="h-full" aria-hidden="true" />
  }

  return (
    <ConversationShell
      topBanner={
        isSilentModelSwitch ? null : (
          <SessionConfigStaleBanner contextKey={tabId} />
        )
      }
      status={visibleConnStatus}
      promptCapabilities={conn.promptCapabilities}
      defaultPath={workingDirForConnection}
      agentName={getAgentDisplayName(selectedAgent)}
      error={visibleConnError}
      claudeApiRetry={conn.claudeApiRetry}
      sessionFailures={conn.sessionFailures}
      onSessionFailureAction={
        conn.connectionId !== null && !conn.isViewer
          ? handleSessionFailureAction
          : undefined
      }
      onSessionFailureDismiss={handleSessionFailureDismiss}
      pendingPermission={conn.pendingPermission}
      pendingQuestion={conn.pendingQuestion}
      pendingAskQuestion={conn.pendingAskQuestion}
      pendingChannelConfirmation={conn.pendingChannelConfirmation}
      autoContinuation={visibleAutoContinuation}
      onAutoContinuationContinue={handleAutoContinuationContinue}
      onAutoContinuationStop={handleAutoContinuationStop}
      onFocus={handleFocus}
      onSend={handleSend}
      onCancel={handleCancel}
      onRespondPermission={handleRespondPermission}
      onAnswerQuestion={handleAnswerQuestion}
      onAnswerAskQuestion={handleAnswerAskQuestion}
      onRespondChannelConfirmation={handleRespondChannelConfirmation}
      modes={connectionModes}
      configOptions={connectionConfigOptions}
      selectedModeId={selectedModeId}
      onModeChange={handleModeChange}
      onConfigOptionChange={handleConfigOptionChange}
      agentType={selectedAgent}
      availableCommands={connectionCommands}
      attachmentTabId={tabId}
      stageAttachmentsInWorkingDir={ownTab?.isChat === true}
      draftStorageKey={draftStorageKey}
      onEphemeralDraftChange={setHasEphemeralDraft}
      hideInput={isWelcomeMode || Boolean(acpLoadError)}
      feedbackList={
        feedback.showList ? (
          <FeedbackNotesDisplay notes={feedback.notes} />
        ) : null
      }
      onAddFeedback={feedback.featureEnabled ? feedback.openDialog : undefined}
      feedbackAddDisabled={!feedback.canSubmit}
      agentInputs={conn.agentInputs}
      onDeleteAgentInput={handleDeleteAgentInput}
      onRetryAgentInput={handleRetryAgentInput}
      onReorderAgentInputs={handleReorderAgentInputs}
      onForceAgentInputsThrough={handleForceAgentInputsThrough}
      isActive={isActive}
      showActiveFlow={showActiveFlow}
      queue={msgQueue}
      onEnqueue={handlePromptingSubmit}
      onQueueReorder={mqReorder}
      onQueueEdit={handleQueueEdit}
      onQueueDelete={mqRemove}
      onQueueRetry={handleQueueRetry}
      editingItemId={mqEditingItemId}
      editingDraftText={editingQueueDraftText}
      editingDraftBlocks={editingQueueDraftBlocks}
      isEditingQueueItem={mqEditingItemId != null}
      onSaveQueueEdit={handleSaveQueueEdit}
      onCancelQueueEdit={handleQueueCancelEdit}
      onForkSend={
        connStatus === "connected" &&
        hasPersistedConversation &&
        conn.supportsFork &&
        !forkSendBlockedByQueue(msgQueue.length)
          ? handleForkSend
          : undefined
      }
    >
      {isWelcomeMode ? (
        <div className="relative isolate flex h-full min-h-0 flex-col overflow-x-hidden overflow-y-auto">
          <div className="flex-[1.18]" />
          <div className="mx-auto flex w-full max-w-4xl shrink-0 flex-col gap-6 px-4 py-4">
            <WelcomeHero />
            <QuickActions onSelect={handleQuickAction} />
            <div className="flex justify-center">
              <AgentSelector
                align="center"
                defaultAgentType={selectedAgent}
                onSelect={handleAgentSelect}
                onFallback={handleAgentFallback}
                onOpenAgentsSettings={handleOpenAgentsSettings}
                disabled={hasSentMessage || dbConversationId != null}
              />
            </div>
            {!isSilentModelSwitch && (autoConnectError || agentConnectError) ? (
              <button
                type="button"
                onClick={handleOpenAgentsSettings}
                className="w-full cursor-pointer rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-center text-xs text-destructive transition-colors hover:bg-destructive/10"
              >
                <div
                  className="overflow-hidden text-ellipsis whitespace-nowrap text-center"
                  title={autoConnectError ?? agentConnectError ?? ""}
                >
                  {autoConnectError ?? agentConnectError}
                </div>
              </button>
            ) : null}
            <ChatInput
              // composerConnStatus (not connStatus): a chat draft mid-reconnect
              // reads "connecting" until the connection's cwd matches, while
              // submissions continue entering this tab's queue.
              status={composerConnStatus}
              promptCapabilities={conn.promptCapabilities}
              defaultPath={workingDirForConnection}
              agentName={getAgentDisplayName(selectedAgent)}
              onFocus={handleFocus}
              onSend={handleSend}
              onCancel={handleCancel}
              responseStarted={hasLiveResponseContent}
              modes={connectionModes}
              configOptions={connectionConfigOptions}
              selectedModeId={selectedModeId}
              onModeChange={handleModeChange}
              onConfigOptionChange={handleConfigOptionChange}
              agentType={selectedAgent}
              availableCommands={connectionCommands}
              attachmentTabId={tabId}
              stageAttachmentsInWorkingDir={ownTab?.isChat === true}
              draftStorageKey={draftStorageKey}
              onEphemeralDraftChange={setHasEphemeralDraft}
              isActive={isActive}
              showActiveFlow={showActiveFlow}
              queue={msgQueue}
              onEnqueue={handlePromptingSubmit}
              onQueueReorder={mqReorder}
              onQueueEdit={handleQueueEdit}
              onQueueDelete={mqRemove}
              onQueueRetry={handleQueueRetry}
              editingItemId={mqEditingItemId}
              editingDraftText={editingQueueDraftText}
              editingDraftBlocks={editingQueueDraftBlocks}
              isEditingQueueItem={mqEditingItemId != null}
              onSaveQueueEdit={handleSaveQueueEdit}
              onCancelQueueEdit={handleQueueCancelEdit}
              onAddFeedback={
                feedback.featureEnabled ? feedback.openDialog : undefined
              }
              feedbackAddDisabled={!feedback.canSubmit}
              injectContent={quickActionInject}
              onInjectConsumed={handleQuickActionConsumed}
              flush
              tall
            />
          </div>
          <div className="flex-[0.82]" />
        </div>
      ) : showDraftHeader ? (
        <div className="flex h-full min-h-0 flex-col">
          <div className="px-4 pt-3 pb-2">
            <AgentSelector
              defaultAgentType={selectedAgent}
              onSelect={handleAgentSelect}
              onFallback={handleAgentFallback}
              onOpenAgentsSettings={handleOpenAgentsSettings}
              disabled={hasSentMessage || dbConversationId != null}
              variant="settings"
            />
            {autoConnectError || agentConnectError ? (
              <button
                type="button"
                onClick={handleOpenAgentsSettings}
                className="mt-2 w-full cursor-pointer rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-center text-xs text-destructive transition-colors hover:bg-destructive/10"
              >
                <div
                  className="overflow-hidden text-ellipsis whitespace-nowrap text-center"
                  title={autoConnectError ?? agentConnectError ?? ""}
                >
                  {autoConnectError ?? agentConnectError}
                </div>
              </button>
            ) : null}
          </div>
          <div className="min-h-0 flex-1">{messageListNode}</div>
        </div>
      ) : (
        messageListNode
      )}
      <FeedbackDialog
        open={feedback.dialogOpen}
        onOpenChange={(open) => {
          if (open) feedback.openDialog()
          else feedback.closeDialog()
        }}
        onSubmit={feedback.submit}
        submitting={feedback.submitting}
        agentName={getAgentDisplayName(selectedAgent)}
      />
      <ConversationPointsDialog
        reason={pointsDialogReason}
        onDismiss={() => setPointsDialogReason(null)}
      />
    </ConversationShell>
  )
})

interface ConversationTabMountProps {
  tab: TabItem
  groupId: string
  index: number
  selectedTabId: string | undefined
  activeTabId: string | null
  groupVisible: boolean
  canTile: boolean
  multipleVisible: boolean
  folderPath?: string
  reloadSignal: number
  setTabRef: (tabId: string, element: HTMLDivElement | null) => void
  switchTab: (tabId: string) => void
}

function ConversationTabMount({
  tab,
  groupId,
  index,
  selectedTabId,
  activeTabId,
  groupVisible,
  canTile,
  multipleVisible,
  folderPath,
  reloadSignal,
  setTabRef,
  switchTab,
}: ConversationTabMountProps) {
  const t = useTranslations("Folder.conversation")
  const active = tab.id === activeTabId
  const visible = canTile || tab.id === selectedTabId
  return (
    <div
      ref={(element) => setTabRef(tab.id, element)}
      className={cn(
        canTile
          ? cn(
              "relative h-full min-w-[24rem] flex-1 overflow-hidden",
              index > 0 && "border-l border-border"
            )
          : visible
            ? "h-full"
            : "absolute inset-0 invisible pointer-events-none"
      )}
      onPointerDownCapture={
        visible && !active ? () => switchTab(tab.id) : undefined
      }
    >
      {multipleVisible && active && (
        <span className="sr-only">{t("activeConversationIndicator")}</span>
      )}
      <ConversationTabView
        tabId={tab.id}
        conversationId={tab.conversationId}
        agentType={tab.agentType}
        workingDir={tab.workingDir ?? folderPath}
        isActive={active}
        isVisible={groupVisible && visible}
        showActiveFlow={multipleVisible && active}
        reloadSignal={reloadSignal}
        groupId={groupId}
      />
    </div>
  )
}

interface ConversationGroupShellProps {
  groupId: string
  rect: GroupRect
  tabs: TabItem[]
  selectedTabId: string | undefined
  activeTabId: string | null
  groupVisible: boolean
  showStrip: boolean
  tileMode: boolean
  dragOver: boolean
  folderPaths: ReadonlyMap<number, string>
  reloadByTabId: Record<string, number>
  setTabRef: (tabId: string, element: HTMLDivElement | null) => void
  switchTab: (tabId: string) => void
}

const FULL_GROUP_RECT: GroupRect = { x: 0, y: 0, w: 100, h: 100 }

function ConversationGroupShell({
  groupId,
  rect,
  tabs,
  selectedTabId,
  activeTabId,
  groupVisible,
  showStrip,
  tileMode,
  dragOver,
  folderPaths,
  reloadByTabId,
  setTabRef,
  switchTab,
}: ConversationGroupShellProps) {
  const canTile = tileMode && tabs.length > 1
  return (
    <div
      data-conv-group-shell={groupId}
      className={cn(
        "absolute flex min-h-0 flex-col overflow-hidden",
        !groupVisible && "invisible pointer-events-none"
      )}
      style={{
        left: `${rect.x}%`,
        top: `${rect.y}%`,
        width: `${rect.w}%`,
        height: `${rect.h}%`,
      }}
    >
      <div className={showStrip ? "h-10 shrink-0" : "hidden"}>
        {showStrip && <TabBar groupId={groupId} />}
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden">
        <ScrollArea
          x={canTile ? "scroll" : "hidden"}
          y="hidden"
          className="h-full w-full"
        >
          <div
            className={cn(
              "relative h-full",
              canTile && "flex min-w-full flex-row"
            )}
          >
            {tabs.map((tab, index) => (
              <ConversationTabMount
                key={tab.id}
                tab={tab}
                groupId={groupId}
                index={index}
                selectedTabId={selectedTabId}
                activeTabId={activeTabId}
                groupVisible={groupVisible}
                canTile={canTile}
                multipleVisible={showStrip || canTile}
                folderPath={folderPaths.get(tab.folderId)}
                reloadSignal={reloadByTabId[tab.id] ?? 0}
                setTabRef={setTabRef}
                switchTab={switchTab}
              />
            ))}
          </div>
        </ScrollArea>
        {dragOver && (
          <div className="pointer-events-none absolute inset-0 z-20 bg-primary/5 ring-2 ring-inset ring-primary/30" />
        )}
      </div>
    </div>
  )
}

export function ConversationDetailPanel() {
  const t = useTranslations("Folder.conversation")
  const tStatus = useTranslations("Folder.statusLabels")
  const tExport = useTranslations("Folder.conversation.exportLabels")
  const tDetails = useTranslations("Folder.sessionDetails")
  const {
    completeTurn: runtimeCompleteTurn,
    removeConversation: runtimeRemoveConversation,
  } = useConversationRuntimeActions()
  const { activeFolder: folder } = useActiveFolder()
  const conversations = useAppWorkspaceStore((s) => s.conversations)
  const allFolders = useAppWorkspaceStore((s) => s.allFolders)
  const tabs = useTabStore((s) => s.tabs)
  const activeTabId = useTabStore((s) => s.activeTabId)
  const groupLayout = useTabStore((s) => s.groupLayout)
  const groupOf = useTabStore((s) => s.groupOf)
  const groupSelection = useTabStore((s) => s.groupSelection)
  const tileByGroup = useTabStore((s) => s.tileByGroup)
  const dragOverGroupId = useTabStore((s) => s.tabDrag?.overGroupId ?? null)
  const {
    openNewConversationTab,
    closeTab,
    switchTab,
    onPreviewTabReplaced,
    resizeGroupSplit,
    endTabDrag,
  } = useTabActions()
  const isMobile = useIsMobile()
  const { isConversations } = useWorkbenchRoute()
  const newConversation = useMemo(() => {
    const activeTab = tabs.find((tab) => tab.id === activeTabId)
    if (!activeTab || activeTab.conversationId != null) return null
    const workingDir = activeTab.workingDir ?? folder?.path
    if (!workingDir) return null
    return { workingDir, folderId: activeTab.folderId }
  }, [tabs, activeTabId, folder?.path])
  const { disconnect: disconnectByKey } = useAcpActions()
  const { addTask, updateTask } = useTaskContext()
  const [reloadByTabId, setReloadByTabId] = useState<Record<string, number>>({})
  const [detailsOpen, setDetailsOpen] = useState(false)

  const exportLabels = useMemo<ExportLabels>(
    () => ({
      untitledConversation: tExport("untitledConversation"),
      agent: tExport("agent"),
      model: tExport("model"),
      status: tExport("status"),
      started: tExport("started"),
      updated: tExport("updated"),
      tokens: tExport("tokens"),
      duration: tExport("duration"),
      inputTokens: tExport("inputTokens"),
      outputTokens: tExport("outputTokens"),
      cacheRead: tExport("cacheRead"),
      cacheWrite: tExport("cacheWrite"),
      user: tExport("user"),
      assistant: tExport("assistant"),
      system: tExport("system"),
      toolResult: tExport("toolResult"),
      toolError: tExport("toolError"),
      statusLabels: {
        in_progress: tStatus("in_progress"),
        pending_review: tStatus("pending_review"),
        completed: tStatus("completed"),
        cancelled: tStatus("cancelled"),
      },
    }),
    [tExport, tStatus]
  )

  // Disconnect the old connection immediately when a preview tab is replaced
  useEffect(() => {
    return onPreviewTabReplaced((replacedTabId) => {
      disconnectByKey(replacedTabId).catch(() => {})
    })
  }, [onPreviewTabReplaced, disconnectByKey])

  // Background turn_complete handler: for conversations not open in tabs.
  // Subscribes via the context's primary `acp://event` listener (single
  // physical Tauri/WebSocket subscription, plus seq dedup from Phase 3b).
  // `useAcpEvent` stabilizes handler identity internally, so the callback
  // can read closure values directly — no caller-side refs needed.
  useAcpEvent(
    useCallback(
      (envelope: EventEnvelope) => {
        if (envelope.type !== "turn_complete") return

        const runtimeConversationId = getConversationIdByExternalIdFromStore(
          envelope.session_id
        )
        // Event-time read: fresher than a render capture ("`conversations`
        // may lag the tab update on fast turns" below applies to the render
        // snapshot; getState() narrows that window).
        const summary = useAppWorkspaceStore
          .getState()
          .conversations.find(
            (item) => item.external_id === envelope.session_id
          )
        const matchedConversationId =
          runtimeConversationId ?? summary?.id ?? null
        if (!matchedConversationId) return

        // Match against every identifier the panel may carry for the same
        // runtime session — otherwise this background handler races the
        // panel's own completeTurn effect and double-promotes streamingTurns
        // into localTurns (visible as a duplicated assistant message until
        // the conversation is reopened from DB).
        //
        // Invariant: `tab.runtimeConversationId` is only set when the panel's
        // effectiveConversationId differs from its bound conversationId, i.e.
        // for new conversations whose session lives under a virtual (negative)
        // id. `dbId2` is always a real DB id, so a runtimeConversationId vs.
        // dbId2 comparison is unreachable and intentionally omitted.
        // `conversations` may lag the tab update on fast turns, so dbId2
        // alone (without the runtime id branch) is not a reliable signal.
        const dbId2 = summary?.id
        const isOpenInTabs = tabs.some(
          (tab) =>
            tab.conversationId === matchedConversationId ||
            tab.runtimeConversationId === matchedConversationId ||
            (dbId2 != null && tab.conversationId === dbId2)
        )
        if (isOpenInTabs) return

        // Promote liveMessage + optimisticTurns to localTurns immediately
        runtimeCompleteTurn(matchedConversationId)

        // If tab was closed while agent was responding, clean up now.
        // Event-time read: fresh via getState(), no reactive subscription.
        const session = getRuntimeSession(matchedConversationId)
        if (session?.pendingCleanup) {
          runtimeRemoveConversation(matchedConversationId)
        }
      },
      [tabs, runtimeCompleteTurn, runtimeRemoveConversation]
    )
  )

  const hasNoTabs = tabs.length === 0 && !activeTabId
  const activeConversationTab = useMemo(
    () =>
      tabs.find(
        (tab) => tab.id === activeTabId && tab.conversationId != null
      ) ?? null,
    [tabs, activeTabId]
  )
  const canReloadActiveConversation = activeConversationTab != null
  const handleReloadActiveConversation = useCallback(() => {
    if (!activeConversationTab) return
    setReloadByTabId((prev) => ({
      ...prev,
      [activeConversationTab.id]: (prev[activeConversationTab.id] ?? 0) + 1,
    }))
  }, [activeConversationTab])

  const [contextMenuSelectedText, setContextMenuSelectedText] = useState("")
  const savedSelectionRangeRef = useRef<Range | null>(null)
  const isContextMenuOpenRef = useRef(false)

  const handleContextMenuOpenChange = useCallback((open: boolean) => {
    isContextMenuOpenRef.current = open
    if (!open) {
      savedSelectionRangeRef.current = null
      return
    }
    const selection = window.getSelection()
    const text = selection?.toString() ?? ""
    setContextMenuSelectedText(text)
    savedSelectionRangeRef.current =
      selection && selection.rangeCount > 0 && !selection.isCollapsed
        ? selection.getRangeAt(0).cloneRange()
        : null
  }, [])

  const handleContextMenuTriggerPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 2) return
      const selection = window.getSelection()
      if (selection && !selection.isCollapsed) {
        event.preventDefault()
      }
    },
    []
  )

  useEffect(() => {
    const handler = () => {
      if (!isContextMenuOpenRef.current) return
      const range = savedSelectionRangeRef.current
      if (!range) return
      if (
        !document.contains(range.startContainer) ||
        !document.contains(range.endContainer)
      ) {
        savedSelectionRangeRef.current = null
        return
      }
      const selection = window.getSelection()
      if (!selection) return
      if (selection.toString().length > 0) return
      selection.removeAllRanges()
      selection.addRange(range)
    }
    document.addEventListener("selectionchange", handler)
    return () => document.removeEventListener("selectionchange", handler)
  }, [])

  const handleCopySelectedText = useCallback(async () => {
    if (!contextMenuSelectedText) return
    const ok = await copyTextFromMenu(contextMenuSelectedText)
    if (ok) {
      toast.success(t("copyTextSuccess"))
    } else {
      toast.error(t("copyTextFailed"))
    }
  }, [contextMenuSelectedText, t])

  const handleNewConversation = useCallback(() => {
    if (!folder) return
    // Right-click "new conversation" inside a conversation tab: keep the
    // active agent when the target folder has no pinned default.
    openNewConversationTab(folder.id, folder.path, { inheritFromActive: true })
  }, [folder, openNewConversationTab])

  const handleCloseActiveTab = useCallback(() => {
    if (!activeTabId) return
    closeTab(activeTabId)
  }, [activeTabId, closeTab])

  // Narrow reactive reads for the ACTIVE conversation only — a background
  // conversation's streaming token no longer re-renders this panel. `canExport`
  // keys on the tab's persisted `conversationId`; the session-details
  // resolution keys on `runtimeConversationId ?? conversationId` (a brand-new
  // conversation streams under a virtual runtime id whose live stats differ), so
  // the two are subscribed SEPARATELY — collapsing them to one lookup would
  // diverge during the virtual→persisted reconciliation window.
  const activeExportConversationId =
    activeConversationTab?.conversationId ?? null
  const canExport = useConversationRuntimeStore(
    (s) =>
      activeExportConversationId != null &&
      s.byConversationId.get(activeExportConversationId)?.detail != null
  )

  // Resolve the active conversation's summary + live token usage the same way
  // the tab view renders them — a new conversation streams under a virtual
  // `runtimeConversationId` with its usage on `sessionStats`. Extracted so the
  // resolution is unit-tested (see active-session-details.test.ts).
  const activeRuntimeId =
    activeConversationTab?.runtimeConversationId ??
    activeConversationTab?.conversationId ??
    null
  const activeRuntimeSession = useConversationRuntimeStore((s) =>
    activeRuntimeId != null
      ? (s.byConversationId.get(activeRuntimeId) ?? null)
      : null
  )
  const {
    summary: activeSessionSummary,
    stats: activeSessionStats,
    model: activeSessionModel,
  } = resolveActiveSessionDetails(
    activeConversationTab,
    // resolveActiveSessionDetails reads only `getSession(runtimeId)`, and its
    // internal `runtimeId` equals `activeRuntimeId` (identical computation), so
    // resolving that single pre-selected session is exact.
    (id) => (id === activeRuntimeId ? activeRuntimeSession : null),
    conversations
  )

  const getExportData = useCallback(() => {
    if (!activeConversationTab?.conversationId) return null
    const session = getRuntimeSession(activeConversationTab.conversationId)
    if (!session?.detail) return null
    return {
      summary: session.detail.summary,
      turns: session.detail.turns,
      sessionStats: session.detail.session_stats,
      labels: exportLabels,
    }
  }, [activeConversationTab, exportLabels])

  const handleExportMarkdown = useCallback(async () => {
    const data = getExportData()
    if (!data) return
    try {
      const result = await exportAsMarkdown(data)
      if (result === "saved") toast.success(t("exportSuccess"))
      // "cancelled": user dismissed the Save dialog — stay silent,
      // matching the downloadImage / workspace-download conventions.
    } catch (err) {
      toast.error(t("exportFailed"))
      console.error("[ConversationDetailPanel] export markdown:", err)
    }
  }, [getExportData, t])

  const handleExportHtml = useCallback(async () => {
    const data = getExportData()
    if (!data) return
    try {
      const result = await exportAsHtml(data)
      if (result === "saved") toast.success(t("exportSuccess"))
    } catch (err) {
      toast.error(t("exportFailed"))
      console.error("[ConversationDetailPanel] export html:", err)
    }
  }, [getExportData, t])

  const handleExportImage = useCallback(async () => {
    const data = getExportData()
    if (!data) return
    const taskId = `export-image-${Date.now()}`
    addTask(taskId, t("exportImage"))
    updateTask(taskId, { status: "running" })
    try {
      const result = await exportAsImage(data)
      updateTask(taskId, { status: "completed" })
      if (result === "saved") toast.success(t("exportSuccess"))
    } catch (err) {
      updateTask(taskId, { status: "failed" })
      if (err instanceof ExportTooLongError) {
        toast.error(t("exportImageTooLong"))
      } else {
        toast.error(t("exportFailed"))
      }
      console.error("[ConversationDetailPanel] export image:", err)
    }
  }, [getExportData, t, addTask, updateTask])

  // Ensure no-tab state is immediately bridged to a real new-conversation tab.
  useEffect(() => {
    if (!folder) return

    if (hasNoTabs) {
      openNewConversationTab(
        folder.id,
        newConversation?.workingDir ?? folder.path
      )
    }
  }, [folder, hasNoTabs, newConversation?.workingDir, openNewConversationTab])

  const { groups: groupRects, handles: groupHandles } = useMemo(
    () => computeRects(groupLayout),
    [groupLayout]
  )
  const orderedGroupIds = useMemo(() => leafIds(groupLayout), [groupLayout])
  const isSplit = orderedGroupIds.length > 1
  const desktopSplit = isSplit && !isMobile
  const activeGroupId = activeTabId
    ? groupOfTab(groupOf, groupLayout, activeTabId)
    : orderedGroupIds[0]
  const tabsByGroup = useMemo(() => {
    const grouped = new Map<string, TabItem[]>()
    for (const groupId of orderedGroupIds) grouped.set(groupId, [])
    for (const tab of tabs) {
      const groupId = groupOfTab(groupOf, groupLayout, tab.id)
      grouped.get(groupId)?.push(tab)
    }
    return grouped
  }, [groupLayout, groupOf, orderedGroupIds, tabs])
  const folderPaths = useMemo(
    () => new Map(allFolders.map((item) => [item.id, item.path])),
    [allFolders]
  )
  const tileTabRefs = useRef<Map<string, HTMLDivElement | null>>(new Map())
  const groupContainerRef = useRef<HTMLDivElement | null>(null)
  const setTabRef = useCallback(
    (tabId: string, element: HTMLDivElement | null) => {
      if (element) tileTabRefs.current.set(tabId, element)
      else tileTabRefs.current.delete(tabId)
    },
    []
  )

  useEffect(() => {
    for (const groupId of orderedGroupIds) {
      if (isMobile && groupId !== activeGroupId) continue
      if (!tileByGroup[groupId]) continue
      const selected = groupSelection[groupId]
      if ((tabsByGroup.get(groupId)?.length ?? 0) < 2 || !selected) continue
      tileTabRefs.current.get(selected)?.scrollIntoView({
        behavior: "smooth",
        inline: "center",
        block: "nearest",
      })
    }
  }, [
    activeGroupId,
    groupSelection,
    isMobile,
    orderedGroupIds,
    tabsByGroup,
    tileByGroup,
  ])

  useEffect(() => {
    const drag = useTabStore.getState().tabDrag
    if (drag && !tabs.some((tab) => tab.id === drag.tabId)) endTabDrag()
  }, [endTabDrag, tabs])

  if (hasNoTabs) {
    return null
  }

  return (
    <>
      <ContextMenu onOpenChange={handleContextMenuOpenChange}>
        <ContextMenuTrigger asChild>
          <div
            ref={groupContainerRef}
            className="relative h-full min-h-0 overflow-hidden"
            onPointerDown={handleContextMenuTriggerPointerDown}
          >
            {orderedGroupIds.map((groupId) => (
              <ConversationGroupShell
                key={groupId}
                groupId={groupId}
                rect={
                  isMobile
                    ? FULL_GROUP_RECT
                    : (groupRects.get(groupId) ?? FULL_GROUP_RECT)
                }
                tabs={tabsByGroup.get(groupId) ?? []}
                selectedTabId={groupSelection[groupId]}
                activeTabId={activeTabId}
                groupVisible={
                  isConversations && (!isMobile || groupId === activeGroupId)
                }
                showStrip={desktopSplit}
                tileMode={!!tileByGroup[groupId]}
                dragOver={dragOverGroupId === groupId}
                folderPaths={folderPaths}
                reloadByTabId={reloadByTabId}
                setTabRef={setTabRef}
                switchTab={switchTab}
              />
            ))}
            {desktopSplit &&
              groupHandles.map((handle) => (
                <GroupSplitHandle
                  key={`${handle.splitId}:${handle.index}`}
                  handle={handle}
                  containerRef={groupContainerRef}
                  onResize={resizeGroupSplit}
                />
              ))}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem
            disabled={!contextMenuSelectedText}
            onSelect={handleCopySelectedText}
          >
            <Copy className="h-4 w-4" />
            {t("copyText")}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            disabled={!folder?.path}
            onSelect={handleNewConversation}
          >
            <SquarePen className="h-4 w-4" />
            {t("newConversation")}
          </ContextMenuItem>
          <ContextMenuSub>
            <ContextMenuSubTrigger disabled={!canExport}>
              <Download className="h-4 w-4" />
              {t("exportConversation")}
            </ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuItem onSelect={handleExportImage}>
                <FileImage className="h-4 w-4" />
                {t("exportImage")}
              </ContextMenuItem>
              <ContextMenuItem onSelect={handleExportMarkdown}>
                <FileText className="h-4 w-4" />
                {t("exportMarkdown")}
              </ContextMenuItem>
              <ContextMenuItem onSelect={handleExportHtml}>
                <FileCode className="h-4 w-4" />
                {t("exportHtml")}
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuItem
            disabled={!canReloadActiveConversation}
            onSelect={handleReloadActiveConversation}
          >
            <RefreshCw className="h-4 w-4" />
            {t("reload")}
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!activeSessionSummary}
            onSelect={() => setDetailsOpen(true)}
          >
            <Info className="h-4 w-4" />
            {tDetails("menuLabel")}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            disabled={!activeTabId}
            onSelect={handleCloseActiveTab}
          >
            <X className="h-4 w-4" />
            {t("closeConversation")}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {activeSessionSummary && (
        <SessionDetailsDialog
          open={detailsOpen}
          onOpenChange={setDetailsOpen}
          summary={activeSessionSummary}
          stats={activeSessionStats}
          model={activeSessionModel}
        />
      )}
    </>
  )
}
