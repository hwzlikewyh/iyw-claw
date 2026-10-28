"use client"

import { memo } from "react"
import { useTranslations } from "next-intl"
import type {
  AgentType,
  ConnectionStatus,
  PromptCapabilitiesInfo,
  PromptDraft,
  PromptInputBlock,
  SessionConfigOptionInfo,
  SessionModeInfo,
  SessionStats,
  AvailableCommandInfo,
} from "@/lib/types"
import type { QueuedMessage } from "@/hooks/use-message-queue"
import {
  MessageInput,
  type ComposerInjectContent,
} from "@/components/chat/message-input"
import { MessageQueueDisplay } from "@/components/chat/message-queue-display"
import { QueuedSendStatus } from "@/components/chat/queued-send-status"
import { cn } from "@/lib/utils"
import { useAccountChatRequestsBlocked } from "@/components/account/use-account-request-guard"

interface ChatInputProps {
  placeholder?: string
  status: ConnectionStatus | null
  promptCapabilities: PromptCapabilitiesInfo
  defaultPath?: string
  agentName?: string
  onFocus?: () => void
  onSend: (
    draft: PromptDraft,
    modeId?: string | null
  ) => boolean | void | Promise<boolean>
  onCancel: () => void
  modes?: SessionModeInfo[]
  configOptions?: SessionConfigOptionInfo[]
  modeLoading?: boolean
  configOptionsLoading?: boolean
  selectedModeId?: string | null
  onModeChange?: (modeId: string) => void
  onConfigOptionChange?: (
    configId: string,
    valueId: string,
    behaviorModelId?: string
  ) => void
  onModelListOpen?: () => void
  agentType?: AgentType | null
  usageStats?: SessionStats | null
  availableCommands?: AvailableCommandInfo[] | null
  attachmentTabId?: string | null
  stageAttachmentsInWorkingDir?: boolean
  draftStorageKey?: string | null
  onEphemeralDraftChange?: (hasEphemeralDraft: boolean) => void
  isActive?: boolean
  /** Show the composer's flowing active-session border. Set only for the active
   *  tab when tiled across multiple sessions; passed through to MessageInput. */
  showActiveFlow?: boolean
  queue?: QueuedMessage[]
  onEnqueue?: (
    draft: PromptDraft,
    modeId: string | null
  ) => boolean | void | Promise<boolean>
  onQueueReorder?: (items: QueuedMessage[]) => void
  onQueueEdit?: (id: string) => void
  onQueueDelete?: (id: string) => void
  onQueueRetry?: (id: string) => void
  editingItemId?: string | null
  editingDraftText?: string | null
  editingDraftBlocks?: PromptInputBlock[] | null
  isEditingQueueItem?: boolean
  onSaveQueueEdit?: (draft: PromptDraft) => void
  onCancelQueueEdit?: () => void
  onForkSend?: (draft: PromptDraft, modeId?: string | null) => boolean | void
  onSideQuestion?: (question: string) => boolean
  onAddFeedback?: () => void
  feedbackAddDisabled?: boolean
  /** Keep the composer usable while the selected Agent connects silently. */
  allowOfflineCompose?: boolean
  blockOfflineSend?: boolean
  /** Retained for caller compatibility; the prompting label is phase-neutral. */
  responseStarted?: boolean
  injectContent?: ComposerInjectContent | null
  onInjectConsumed?: () => void
  /** Drop the input's own horizontal padding when an ancestor already supplies
   *  the gutter (the welcome column wraps this in its own `px-4`). */
  flush?: boolean
  /** Use a taller minimum height for the composer. Set for the welcome
   *  (new-conversation) composer, which sits in a roomy empty state; active and
   *  historical conversations keep the compact default. */
  tall?: boolean
}

export const ChatInput = memo(function ChatInput({
  status,
  promptCapabilities,
  defaultPath,
  agentName,
  onFocus,
  onSend,
  onCancel,
  modes,
  configOptions,
  modeLoading = false,
  configOptionsLoading = false,
  selectedModeId,
  onModeChange,
  onConfigOptionChange,
  onModelListOpen,
  agentType,
  usageStats,
  availableCommands,
  attachmentTabId,
  stageAttachmentsInWorkingDir,
  draftStorageKey,
  onEphemeralDraftChange,
  isActive,
  showActiveFlow,
  queue,
  onEnqueue,
  onQueueReorder,
  onQueueEdit,
  onQueueDelete,
  onQueueRetry,
  editingItemId,
  editingDraftText,
  editingDraftBlocks,
  isEditingQueueItem,
  onSaveQueueEdit,
  onCancelQueueEdit,
  onForkSend,
  onSideQuestion,
  onAddFeedback,
  feedbackAddDisabled,
  allowOfflineCompose = true,
  blockOfflineSend = false,
  injectContent,
  onInjectConsumed,
  flush = false,
  tall = false,
  placeholder,
}: ChatInputProps) {
  const t = useTranslations("Folder.chat.chatInput")
  const requestsBlocked = useAccountChatRequestsBlocked()
  const isConnected = status === "connected"
  const isPrompting = status === "prompting"
  const sendDisabled = blockOfflineSend && !isConnected && !isPrompting
  const waitingMessageCount = queue?.filter((item) => !item.blocked).length ?? 0
  const resolvedAgentName = agentName ?? "Agent"
  const activityPlaceholder = blockOfflineSend
    ? null
    : status === "connecting"
      ? t("startingKernel", { agent: resolvedAgentName })
      : isPrompting
        ? t("working")
        : null

  return (
    <div
      className={cn("pt-0 pb-1", !flush && "px-4")}
      onContextMenu={(event) => event.stopPropagation()}
    >
      {!isPrompting && !queue?.[0]?.blocked && waitingMessageCount > 0 && (
        <QueuedSendStatus count={waitingMessageCount} />
      )}
      {queue &&
        queue.length > 0 &&
        onQueueReorder &&
        onQueueEdit &&
        onQueueDelete && (
          <MessageQueueDisplay
            queue={queue}
            onReorder={onQueueReorder}
            onEdit={onQueueEdit}
            onDelete={onQueueDelete}
            onRetry={onQueueRetry ?? (() => {})}
            editingItemId={editingItemId ?? null}
          />
        )}
      <MessageInput
        onSend={(draft, mode) =>
          requestsBlocked || sendDisabled ? false : onSend(draft, mode)
        }
        promptCapabilities={promptCapabilities}
        onFocus={onFocus}
        defaultPath={defaultPath}
        disabled={allowOfflineCompose ? false : !isConnected && !isPrompting}
        sendDisabled={sendDisabled}
        isPrompting={isPrompting}
        animatePlaceholder={activityPlaceholder !== null}
        onCancel={onCancel}
        modes={modes}
        configOptions={configOptions}
        usageStats={usageStats}
        modeLoading={modeLoading}
        configOptionsLoading={configOptionsLoading}
        selectedModeId={selectedModeId}
        onModeChange={onModeChange}
        onConfigOptionChange={onConfigOptionChange}
        onModelListOpen={onModelListOpen}
        agentType={agentType}
        availableCommands={availableCommands}
        attachmentTabId={attachmentTabId}
        stageAttachmentsInWorkingDir={stageAttachmentsInWorkingDir}
        draftStorageKey={draftStorageKey}
        onEphemeralDraftChange={onEphemeralDraftChange}
        isActive={isActive}
        showActiveFlow={showActiveFlow}
        hasQueuedMessages={(queue?.length ?? 0) > 0}
        onEnqueue={
          onEnqueue
            ? (draft, mode) =>
                requestsBlocked || sendDisabled ? false : onEnqueue(draft, mode)
            : undefined
        }
        editingItemId={editingItemId}
        editingDraftText={editingDraftText}
        editingDraftBlocks={editingDraftBlocks}
        isEditingQueueItem={isEditingQueueItem}
        onSaveQueueEdit={onSaveQueueEdit}
        onCancelQueueEdit={onCancelQueueEdit}
        onForkSend={
          onForkSend
            ? (draft, mode) =>
                requestsBlocked || sendDisabled
                  ? false
                  : onForkSend(draft, mode)
            : undefined
        }
        onSideQuestion={onSideQuestion}
        onAddFeedback={onAddFeedback}
        feedbackAddDisabled={feedbackAddDisabled}
        injectContent={injectContent}
        onInjectConsumed={onInjectConsumed}
        placeholder={activityPlaceholder ?? placeholder ?? t("sendMessage")}
        className={cn(tall ? "min-h-30" : "min-h-24", "max-h-60")}
      />
    </div>
  )
})

ChatInput.displayName = "ChatInput"
