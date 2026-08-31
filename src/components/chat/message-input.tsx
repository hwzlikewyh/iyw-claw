"use client"

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type SetStateAction,
} from "react"
import { isDesktop, openFileDialog } from "@/lib/platform"
import Image from "next/image"
import { useLocale, useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  BookOpenText,
  Check,
  ChevronUp,
  ClipboardPaste,
  Cog,
  Copy,
  FileImage,
  FileStack,
  FolderSearch,
  GitFork,
  Lock,
  LoaderCircle,
  MessageSquareText,
  Paperclip,
  Plus,
  Scissors,
  Send,
  Sparkles,
  TextSelect,
  RotateCcw,
  Square,
  TriangleAlert,
  Upload,
  X,
} from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
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
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import type { EditorImageResult } from "@/components/image-editor/image-editor-model"
import { cn, copyTextFromMenu, randomUUID } from "@/lib/utils"
import {
  buildDirectoryUri,
  buildFileUri,
  buildFileUriWithRange,
  fileUriToPath,
  formatFileRangeLabel,
  isAbsoluteFilesystemPath,
} from "@/lib/reference-link"
import {
  filesFromClipboard,
  clipboardHasText,
  imageFilesFromClipboardApi,
} from "@/lib/clipboard-images"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import {
  quickMessagesList,
  prepareChatImagePath,
  readFileBase64,
  readLocalFileBase64,
  stageLocalChatAttachment,
  uploadAttachment,
  uploadChatImage,
  uploadLocalChatImagePathToRemote,
  uploadLocalPathToRemote,
  isEmptyAttachmentError,
  type ChatImageStorageOptions,
  type PreparedChatImage,
  CHAT_IMAGE_I18N_KEY_TOO_LARGE,
  UPLOAD_MAX_BYTES,
  UPLOAD_I18N_KEY_TOO_LARGE,
  UPLOAD_I18N_KEY_NOT_A_FILE,
  UPLOAD_I18N_KEY_QUOTA_EXCEEDED,
} from "@/lib/api"
import {
  CHAT_IMAGE_DERIVED_MAX_BYTES,
  CHAT_IMAGE_SOURCE_MAX_BYTES,
} from "@/lib/chat-image"
import { extractAppCommandError } from "@/lib/app-error"
import { getActiveRemoteConnectionId } from "@/lib/transport"
import { ServerFileBrowserDialog } from "@/components/shared/server-file-browser-dialog"
import { toast } from "sonner"
import { preparePickedAttachmentPaths } from "./chat-attachment-staging"
import { disposeTauriListener } from "@/lib/tauri-listener"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import type {
  AgentSkillItem,
  AgentType,
  AvailableCommandInfo,
  ExpertListItem,
  PromptCapabilitiesInfo,
  PromptDraft,
  ScenarioVariable,
  PromptSkillPackage,
  PromptInputBlock,
  QuickMessage,
  SessionConfigOptionInfo,
  SessionModeInfo,
} from "@/lib/types"
import {
  ATTACH_FILE_TO_SESSION_EVENT,
  ATTACH_IMAGE_TO_SESSION_EVENT,
  APPEND_TEXT_TO_SESSION_EVENT,
  type AttachFileToSessionDetail,
  type AttachImageToSessionDetail,
  type AppendTextToSessionDetail,
} from "@/lib/session-attachment-events"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import {
  ConversationContextBar,
  ConversationFolderBranchPicker,
  useConversationFolderBranchPickerVisible,
} from "@/components/chat/conversation-context-bar"
import { SessionUsageChip } from "@/components/layout/status-bar-tokens"
import { InlineModeSelector } from "@/components/chat/mode-selector"
import { InlineSessionConfigSelector } from "@/components/chat/session-config-selector"
import { ModelOptionPicker } from "@/components/chat/model-option-picker"
import {
  SessionSelectorsPanel,
  type SessionSelectorGroup,
  type SessionSelectorSetting,
} from "@/components/chat/session-selectors-panel"
import {
  deriveModelGroups,
  isModelBehaviorConfigOption,
  isModelConfigOption,
  modelListGroups,
} from "@/lib/model-config-groups"
import {
  localizeSessionConfigOption,
  type SessionConfigTranslator,
} from "@/lib/session-config-localization"
import { orderSessionSelectors } from "@/lib/session-selector-order"
import { refreshAgentSkills, useAgentSkills } from "@/hooks/use-agent-skills"
import { useBuiltInExperts } from "@/hooks/use-built-in-experts"
import { useEnabledSkillIds } from "@/hooks/use-enabled-skill-ids"
import { useScrollbarSafeDismiss } from "@/hooks/use-scrollbar-safe-dismiss"
import {
  useRealtimeVoiceInput,
  type RealtimeVoiceErrorKind,
} from "@/hooks/use-realtime-voice-input"
import {
  getExpertIcon,
  isVisibleExpertId,
  pickLocalized,
} from "@/lib/expert-presentation"
import { OFFICE_ACTIONS, type OfficeAction } from "@/lib/office-actions"
import {
  clearMessageInputDraftV2,
  loadMessageInputDraftV2,
  saveMessageInputDraftV2,
  setLiveMessageInputDraftPresence,
} from "@/lib/message-input-draft"
import {
  RichComposer,
  type RichComposerHandle,
} from "@/components/chat/composer/rich-composer"
import { RealtimeVoiceButton } from "@/components/chat/realtime-voice-button"
import {
  composerLeafText,
  docToPromptBlocks,
  serializeDocToDisplayText,
} from "@/components/chat/composer/to-prompt-blocks"
import {
  buildEmbeddedReferenceUri,
  isEmbeddedReferenceUri,
} from "@/components/chat/composer/reference-uri"
import {
  applyTaskReference,
  applyExpertReference,
  clearTaskReference,
  getExpertReference,
  getTaskReference,
  isComposerChromeClick,
  isComposerEmpty,
  isTaskReference,
  normalizeDirectiveReferences,
  restampSkillPrefixes,
  restoreBlocksIntoEditor,
} from "@/components/chat/composer/composer-commands"
import {
  commandToReference,
  skillToReference,
} from "@/components/chat/composer/invocation-reference"
import { cutSelectionToClipboard } from "@/components/chat/composer/clipboard-actions"
import type { ReferenceAttrs } from "@/components/chat/composer/types"
import type { Editor, JSONContent } from "@tiptap/core"
import {
  useReferenceSearch,
  type ReferenceGroupLabels,
} from "@/components/chat/composer/use-reference-search"
import type { MentionUiLabels } from "@/components/chat/composer/suggestion/types"
import type {
  ImageInputAttachment,
  ImageAttachmentStaging,
  InputAttachment,
  ResourceInputAttachment,
} from "./message-input-attachments"
import {
  ProjectReferenceDialog,
  type ProjectReferenceSelection,
} from "@/components/chat/project-reference-dialog"
import {
  TaskCommandMenu,
  TaskModeRail,
} from "@/components/chat/task-command-menu"

/**
 * Payload pushed into the composer from outside (e.g. a welcome-page quick
 * action). `text` replaces the document; `skill`, when present, is prepended as
 * the leading invocation badge (serializes to `${prefix}${id}` as the first
 * token).
 */
export interface ComposerInjectContent {
  text: string
  skill?: { id: string; label: string; package?: PromptSkillPackage }
  scenario?: { variables: ScenarioVariable[] }
}

interface PendingSendSnapshot {
  doc: JSONContent
  attachments: InputAttachment[]
  embeddedPayloads: Map<string, PromptInputBlock>
  composerInstanceId: string
  fallbackText: string
  mutationVersion: number
}

type PendingSendRestoreListener = (snapshot: PendingSendSnapshot) => boolean

const pendingSendRestoreListeners = new Map<
  string,
  Set<PendingSendRestoreListener>
>()
const PENDING_SEND_RESTORE_TTL_MS = 30_000
const pendingSendRestores = new Map<
  string,
  { snapshot: PendingSendSnapshot; cleanupTimer: ReturnType<typeof setTimeout> }
>()

function publishPendingSendRestore(
  scopeKey: string,
  snapshot: PendingSendSnapshot
): void {
  const restored = Array.from(pendingSendRestoreListeners.get(scopeKey) ?? [])
    .reverse()
    .some((listener) => listener(snapshot))
  if (restored) return
  const previous = pendingSendRestores.get(scopeKey)
  if (previous) clearTimeout(previous.cleanupTimer)
  const cleanupTimer = setTimeout(() => {
    const pending = pendingSendRestores.get(scopeKey)
    if (pending?.snapshot === snapshot) pendingSendRestores.delete(scopeKey)
  }, PENDING_SEND_RESTORE_TTL_MS)
  pendingSendRestores.set(scopeKey, { snapshot, cleanupTimer })
}

function subscribePendingSendRestore(
  scopeKey: string,
  listener: PendingSendRestoreListener
): () => void {
  const listeners = pendingSendRestoreListeners.get(scopeKey) ?? new Set()
  listeners.add(listener)
  pendingSendRestoreListeners.set(scopeKey, listeners)
  const pending = pendingSendRestores.get(scopeKey)
  if (pending && listener(pending.snapshot)) {
    clearTimeout(pending.cleanupTimer)
    pendingSendRestores.delete(scopeKey)
  }
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0) pendingSendRestoreListeners.delete(scopeKey)
  }
}

interface MessageInputProps {
  onSend: (
    draft: PromptDraft,
    modeId?: string | null
  ) => boolean | void | Promise<boolean>
  placeholder?: string
  animatePlaceholder?: boolean
  defaultPath?: string
  disabled?: boolean
  autoFocus?: boolean
  onFocus?: () => void
  className?: string
  isPrompting?: boolean
  onCancel?: () => void
  modes?: SessionModeInfo[]
  configOptions?: SessionConfigOptionInfo[]
  modeLoading?: boolean
  configOptionsLoading?: boolean
  selectedModeId?: string | null
  onModeChange?: (modeId: string) => void
  onConfigOptionChange?: (configId: string, valueId: string) => void
  agentType?: AgentType | null
  availableCommands?: AvailableCommandInfo[] | null
  promptCapabilities: PromptCapabilitiesInfo
  attachmentTabId?: string | null
  stageAttachmentsInWorkingDir?: boolean
  draftStorageKey?: string | null
  onEphemeralDraftChange?: (hasEphemeralDraft: boolean) => void
  isActive?: boolean
  /** Paint the flowing active-session gradient on the composer border. Set only
   *  for the active tab while tiled across multiple sessions; a lone or
   *  non-tiled session keeps the plain default border. Independent of
   *  `isActive` (which still drives auto-focus/connect). */
  showActiveFlow?: boolean
  /** Existing queued drafts may run before a new direct send. In that case the
   *  composer must not label the newly queued task as the active turn. */
  hasQueuedMessages?: boolean
  onEnqueue?: (
    draft: PromptDraft,
    modeId: string | null
  ) => boolean | void | Promise<boolean>
  /** Id of the queue item being edited — the stable key for (re)hydration, so
   *  switching between two items with identical display text still reloads. */
  editingItemId?: string | null
  editingDraftText?: string | null
  /**
   * The queued message's full `draft.blocks`, when editing a queue item. Lets
   * the composer restore inline reference badges + attachments (not just text);
   * falls back to {@link editingDraftText} when absent.
   */
  editingDraftBlocks?: PromptInputBlock[] | null
  isEditingQueueItem?: boolean
  onSaveQueueEdit?: (draft: PromptDraft) => void
  onCancelQueueEdit?: () => void
  /** Fork the session and send `draft`. A synchronous false keeps the draft;
   *  accepted attempts clear immediately and the parent re-queues async failures. */
  onForkSend?: (draft: PromptDraft, modeId?: string | null) => boolean | void
  /** Open the live-feedback dialog (from the "+" menu). When omitted the entry
   *  is hidden (feature off). */
  onAddFeedback?: () => void
  /** Grey out the live-feedback "+" entry when a note can't be sent right now
   *  (no active turn / agent lacks the tool). */
  feedbackAddDisabled?: boolean
  injectContent?: ComposerInjectContent | null
  onInjectConsumed?: () => void
}

const MIME_BY_EXT: Record<string, string> = {
  txt: "text/plain",
  md: "text/markdown",
  json: "application/json",
  yaml: "application/yaml",
  yml: "application/yaml",
  csv: "text/csv",
  html: "text/html",
  css: "text/css",
  js: "text/javascript",
  mjs: "text/javascript",
  cjs: "text/javascript",
  ts: "text/typescript",
  tsx: "text/tsx",
  jsx: "text/jsx",
  py: "text/x-python",
  rs: "text/rust",
  go: "text/x-go",
  java: "text/x-java-source",
  xml: "application/xml",
  toml: "application/toml",
  pdf: "application/pdf",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
}

const IMAGE_ATTACHMENT_MAX_BYTES = CHAT_IMAGE_DERIVED_MAX_BYTES
const SUPPORTED_IMAGE_MIME_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/webp",
  "image/gif",
])

function fileNameFromPath(path: string): string {
  return path.split(/[/\\]/).pop() || path
}

function directoryNameFromPath(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, "")
  return trimmed.split(/[/\\]/).pop() || path
}

function mimeTypeFromPath(path: string): string | null {
  const ext = path.split(".").pop()?.toLowerCase() ?? ""
  return MIME_BY_EXT[ext] ?? null
}

function imageMimeTypeForFile(file: File): string | null {
  const declared = file.type.trim().toLowerCase()
  if (declared.startsWith("image/")) {
    return SUPPORTED_IMAGE_MIME_TYPES.has(declared) ? declared : null
  }
  const byName = mimeTypeFromPath(file.name)
  return byName && SUPPORTED_IMAGE_MIME_TYPES.has(byName) ? byName : null
}

function imageMimeTypeFromPath(path: string): string | null {
  const mime = mimeTypeFromPath(path)
  return mime && SUPPORTED_IMAGE_MIME_TYPES.has(mime) ? mime : null
}

function bytesFromBase64(data: string): number {
  const padding = (data.match(/=+$/) ?? [""])[0].length
  return Math.max(0, Math.floor((data.length * 3) / 4) - padding)
}

function isPublicImageUrl(value: string | null | undefined): value is string {
  if (!value) return false
  try {
    const url = new URL(value)
    return (
      url.protocol === "https:" &&
      Boolean(url.hostname) &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash
    )
  } catch {
    return false
  }
}

function imageAttachmentSrc(attachment: ImageInputAttachment): string {
  if (attachment.previewUrl) return attachment.previewUrl
  if (isPublicImageUrl(attachment.uri)) return attachment.uri
  return attachment.data
    ? `data:${attachment.mimeType};base64,${attachment.data}`
    : ""
}

function releaseUnusedImagePreviews(
  current: InputAttachment[],
  next: InputAttachment[]
): void {
  const retained = new Set(
    next.flatMap((item) =>
      item.type === "image" && item.previewUrl ? [item.previewUrl] : []
    )
  )
  for (const item of current) {
    if (
      item.type === "image" &&
      item.previewUrl &&
      !retained.has(item.previewUrl)
    ) {
      if (item.previewUrl.startsWith("blob:")) {
        URL.revokeObjectURL(item.previewUrl)
      }
    }
  }
}

function base64ImageFile(result: EditorImageResult): File {
  const binary = atob(result.data)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index)
  }
  return new File([bytes], result.name, { type: result.mime_type })
}

async function createImagePathPreview(
  path: string,
  source: "local" | "workspace" | "remote-local",
  name: string,
  mimeType: string
): Promise<string | null> {
  try {
    const data =
      source === "workspace"
        ? await readFileBase64(path, CHAT_IMAGE_DERIVED_MAX_BYTES)
        : await readLocalFileBase64(path, CHAT_IMAGE_DERIVED_MAX_BYTES)
    return URL.createObjectURL(
      base64ImageFile({ data, mime_type: mimeType, name })
    )
  } catch {
    return null
  }
}

function restoredImageFile(attachment: ImageInputAttachment): File {
  return base64ImageFile({
    data: attachment.data,
    mime_type: attachment.mimeType,
    name: attachment.name,
  })
}

interface RestoredImageUpload {
  attachment: ImageInputAttachment
  file: File
}

function prepareRestoredImages(restored: InputAttachment[]): {
  images: ImageInputAttachment[]
  uploads: RestoredImageUpload[]
} {
  const uploads: RestoredImageUpload[] = []
  const images = restored
    .filter((item): item is ImageInputAttachment => item.type === "image")
    .map((attachment) => {
      if (isPublicImageUrl(attachment.uri)) {
        return { ...attachment, data: "" }
      }
      if (!attachment.data) return attachment
      try {
        const file = restoredImageFile(attachment)
        const next: ImageInputAttachment = {
          ...attachment,
          staging: {
            status: "uploading",
            source: { kind: "browser-file", file },
          },
        }
        uploads.push({ attachment: next, file })
        return next
      } catch (error) {
        console.error("[MessageInput] restored image decode failed", {
          name: attachment.name,
          error,
        })
        return attachment
      }
    })
  return { images, uploads }
}

function insertRestoredResources(
  editor: Editor,
  restored: InputAttachment[],
  payloads: Map<string, PromptInputBlock>
) {
  const resources = restored.filter(
    (item): item is ResourceInputAttachment => item.type === "resource"
  )
  let chain = editor.chain().focus("end")
  for (const attachment of resources) {
    const refUri = buildEmbeddedReferenceUri()
    const block: PromptInputBlock =
      attachment.kind === "embedded"
        ? {
            type: "resource",
            uri: attachment.uri,
            mime_type: attachment.mimeType,
            text: attachment.text ?? null,
            blob: attachment.blob ?? null,
          }
        : {
            type: "resource_link",
            uri: attachment.uri,
            name: attachment.name,
            mime_type: attachment.mimeType,
            description: null,
          }
    payloads.set(refUri, block)
    chain = chain
      .insertReference({
        refType: "file",
        id: refUri,
        label: attachment.name,
        uri: refUri,
        meta: { fileKind: "file" },
      })
      .insertContent(" ")
  }
  if (resources.length > 0) chain.run()
}

function updateImageAttachment(
  current: InputAttachment[],
  id: string,
  update: (attachment: ImageInputAttachment) => ImageInputAttachment
): InputAttachment[] {
  return current.map((attachment) =>
    attachment.type === "image" && attachment.id === id
      ? update(attachment)
      : attachment
  )
}

function applyPreparedImage(
  current: InputAttachment[],
  id: string,
  prepared: PreparedChatImage,
  sourceMimeType?: string
): InputAttachment[] {
  return updateImageAttachment(current, id, (item) => ({
    ...item,
    data: "",
    uri: prepared.url,
    localPath: prepared.localPath,
    name: prepared.name,
    mimeType: prepared.mimeType,
    sourceMimeType: sourceMimeType ?? item.sourceMimeType,
    previewUrl: undefined,
    staging: undefined,
  }))
}

async function retryImageUpload(
  source: ImageAttachmentStaging["source"],
  options: ChatImageStorageOptions
) {
  if (source.kind === "browser-file") {
    return uploadChatImage(source.file, options)
  }
  if (source.source === "remote-local") {
    return uploadLocalChatImagePathToRemote(source.path, options)
  }
  return prepareChatImagePath(source.path, source.source, options)
}

function imageTooLargeDetails(error: unknown, fallbackName: string) {
  const appError = extractAppCommandError(error)
  if (appError?.i18n_key !== CHAT_IMAGE_I18N_KEY_TOO_LARGE) return null
  return {
    name: appError.i18n_params?.name ?? fallbackName,
    size: appError.i18n_params?.size ?? "100.0",
    limit:
      appError.i18n_params?.limit ??
      String(CHAT_IMAGE_SOURCE_MAX_BYTES / (1024 * 1024)),
  }
}

function hasDragFiles(dataTransfer: DataTransfer | null): boolean {
  if (!dataTransfer?.types) return false
  return Array.from(dataTransfer.types).includes("Files")
}

function pointWithinElement(
  position: { x: number; y: number },
  element: HTMLElement
): boolean {
  // Inactive conversation tabs are kept mounted at `absolute inset-0` with
  // `visibility: hidden` (see ConversationDetailPanel), so their bounding rect
  // overlaps the active tab's. Without this guard every tab's Tauri drag
  // listener would treat the same OS drop as falling inside its own input,
  // and dropped files would silently fan out across every open conversation.
  const style = element.ownerDocument?.defaultView?.getComputedStyle(element)
  if (style) {
    if (
      style.visibility === "hidden" ||
      style.display === "none" ||
      style.pointerEvents === "none"
    ) {
      return false
    }
  }
  const rect = element.getBoundingClientRect()
  if (rect.width === 0 || rect.height === 0) return false
  const dpr = window.devicePixelRatio || 1
  const candidates = [
    { x: position.x, y: position.y },
    { x: position.x / dpr, y: position.y / dpr },
  ]
  return candidates.some(
    (point) =>
      point.x >= rect.left &&
      point.x <= rect.right &&
      point.y >= rect.top &&
      point.y <= rect.bottom
  )
}

function getFilePath(file: File): string | null {
  const withPath = file as File & { path?: string; webkitRelativePath?: string }
  if (typeof withPath.path === "string" && withPath.path.trim().length > 0) {
    return withPath.path
  }
  if (
    typeof withPath.webkitRelativePath === "string" &&
    withPath.webkitRelativePath.trim().length > 0
  ) {
    return withPath.webkitRelativePath
  }
  return null
}

// Non-image files attach as inline file badges in the editor (like `@`-file
// references), not as out-of-band chips. A file with a real `file://` path uses
// that uri directly (it serializes to a ResourceLink and round-trips through the
// draft doc untouched). A path-less file (a local-desktop paste/drop carrying
// inline bytes — an embedded resource or a `data:` link) can't live in the doc,
// so its badge carries an inert `iyw-claw://embedded/<uuid>` display uri
// (`buildEmbeddedReferenceUri`) while the real bytes-bearing block is held in the
// `embeddedPayloadsRef` map keyed by that uri. `docToPromptBlocks` drops the
// embedded badge from the prose; `buildDraft` appends the mapped block for every
// embedded badge still in the document. The `iyw-claw://` scheme is never a real
// path (no collision with a genuine attachment) and survives the transcript's
// sanitize/harden pipeline, so it renders as an inert file badge, not a blocked
// link — see {@link buildEmbeddedReferenceUri} / {@link isEmbeddedReferenceUri}.

/** Whether the document already holds a file reference badge for `uri` (used to
 *  dedupe repeated drops/picks of the same path, mirroring the old seen-set). */
function editorHasFileReference(editor: Editor, uri: string): boolean {
  let found = false
  editor.state.doc.descendants((node) => {
    if (found) return false
    if (
      node.type.name === "reference" &&
      node.attrs?.refType === "file" &&
      node.attrs?.uri === uri
    ) {
      found = true
      return false
    }
    return true
  })
  return found
}

/** Drop embedded-attachment reference badges from a draft document before it is
 *  persisted: their bytes live only in the in-memory `embeddedPayloadsRef` map
 *  (never serialized into the draft), so a restored badge would send nothing.
 *  Identified purely by the unambiguous `iyw-claw://embedded/…` display uri (no map
 *  needed) — a real `file://` attachment is never matched. Stripping at save
 *  keeps the live badge visible this session but matches the pre-existing
 *  behavior where out-of-band pasted bytes don't survive a draft round-trip. */
function stripEmbeddedReferences(doc: JSONContent): JSONContent {
  if (!doc.content) return doc
  const content: JSONContent[] = []
  for (const child of doc.content) {
    if (
      child.type === "reference" &&
      typeof child.attrs?.uri === "string" &&
      isEmbeddedReferenceUri(child.attrs.uri)
    ) {
      continue
    }
    content.push(stripEmbeddedReferences(child))
  }
  return { ...doc, content }
}

function hasEmbeddedReference(doc: JSONContent): boolean {
  if (
    doc.type === "reference" &&
    typeof doc.attrs?.uri === "string" &&
    isEmbeddedReferenceUri(doc.attrs.uri)
  ) {
    return true
  }
  return doc.content?.some(hasEmbeddedReference) ?? false
}

function SelectorLoadingChip({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground">
      <span className="h-1.5 w-1.5 rounded-full bg-primary animate-pulse" />
      <span>{label}</span>
    </div>
  )
}

export function MessageInput({
  onSend,
  placeholder,
  animatePlaceholder = false,
  defaultPath,
  disabled = false,
  autoFocus = false,
  onFocus,
  onCancel,
  className,
  isPrompting = false,
  modes,
  configOptions,
  modeLoading = false,
  configOptionsLoading = false,
  selectedModeId,
  onModeChange,
  onConfigOptionChange,
  agentType,
  availableCommands,
  promptCapabilities,
  attachmentTabId,
  stageAttachmentsInWorkingDir = false,
  draftStorageKey,
  onEphemeralDraftChange,
  isActive = false,
  showActiveFlow = false,
  hasQueuedMessages = false,
  onEnqueue,
  editingItemId,
  editingDraftText,
  editingDraftBlocks,
  isEditingQueueItem = false,
  onSaveQueueEdit,
  onCancelQueueEdit,
  onForkSend,
  injectContent,
  onInjectConsumed,
}: MessageInputProps) {
  const t = useTranslations("Folder.chat.messageInput")
  const { openSkillMarket } = useWorkbenchRoute()
  const tSessionConfig = t as unknown as SessionConfigTranslator
  const tQueue = useTranslations("Folder.chat.messageQueue")
  // Kept as a separate binding from `t` so its call sites — exclusively
  // upload / attachment toasts — read as a single coherent group when
  // scanning the file. Same namespace, no extra runtime cost.
  const tAttach = useTranslations("Folder.chat.messageInput")
  const desktopMode = isDesktop()
  // Cached for the window's lifetime: `getActiveRemoteConnectionId()` is
  // configured once when a remote-workspace window is created and never
  // mutates afterwards. A desktop window bound to a remote iyw-claw-server
  // controls whether selected paths belong to the local agent or must be
  // streamed to the workspace. Both desktop variants use the native picker;
  // remote desktop paths are handed to the remote upload proxy before sending.
  const showNativePaperclip = useMemo(
    () => desktopMode && getActiveRemoteConnectionId() === null,
    [desktopMode]
  )
  const chatImageStorage = useMemo<ChatImageStorageOptions>(
    () => ({
      sessionId: attachmentTabId ?? null,
      chatDir: stageAttachmentsInWorkingDir ? (defaultPath ?? null) : null,
    }),
    [attachmentTabId, defaultPath, stageAttachmentsInWorkingDir]
  )
  // The `$` prefix autocomplete is Codex-only: Codex advertises very few
  // native slash commands, so we augment the dropdown with the agent's
  // skills read from disk. Other agents already surface their full command
  // set through ACP `availableCommands`, so injecting skills there would
  // be duplicate/extra UI noise — skip the skills fetch for them entirely.
  const skillAgentType = agentType === "codex" ? "codex" : null
  // Pass the working dir so we see both global skills and folder-scoped
  // project skills (e.g. `{folder}/.codex/skills`). Without this, users
  // only ever saw global skills in the `$` autocomplete.
  const availableSkills = useAgentSkills(skillAgentType, defaultPath ?? null)
  const visibleAvailableSkills = useMemo(
    () => availableSkills.filter((skill) => isVisibleExpertId(skill.id)),
    [availableSkills]
  )
  // The + menu exposes editable skills enabled for the active agent. Read-only
  // built-ins stay available through their dedicated product entry points.
  const enabledSkills = useAgentSkills(agentType ?? null, defaultPath ?? null)
  const visibleEnabledSkills = useMemo(
    () =>
      enabledSkills.filter(
        (skill) => !skill.read_only && isVisibleExpertId(skill.id)
      ),
    [enabledSkills]
  )
  const skillPrefix = agentType === "codex" ? "$" : "/"
  const { shortcuts } = useShortcutSettings()
  const effectiveDraftStorageKey = draftStorageKey ?? null
  // The "+" menu's expert / daily-office skill shortcuts mirror the welcome-page
  // quick actions: localized labels (`tQa` reads the same namespace those cards
  // use), the bundled experts, and per-agent skill-enabled gating.
  const locale = useLocale()
  const tQa = useTranslations("Folder.chat.welcomePanel.quickActions")
  const experts = useBuiltInExperts()
  const {
    enabledIds,
    ready: skillStatusReady,
    supported: skillManagementSupported,
  } = useEnabledSkillIds(agentType ?? null)
  const editorRef = useRef<RichComposerHandle>(null)
  const composerInstanceIdRef = useRef(randomUUID())
  // The editor owns the content now; this mirror of its empty state drives the
  // send button and `hasSendableContent`.
  const [composerEmpty, setComposerEmpty] = useState(true)
  const [draftTask, setDraftTask] = useState<ReferenceAttrs | null>(null)
  const [runningTask, setRunningTask] = useState<ReferenceAttrs | null>(null)
  // Flips true once the RichComposer's async (immediatelyRender:false) editor has
  // mounted, so the hydration effect can use the imperative handle.
  const [composerReady, setComposerReady] = useState(false)
  const [composerHydrated, setComposerHydrated] = useState(false)
  // `attachments` now holds only images; non-image files live inline as editor
  // reference badges. This map carries the real bytes-bearing block for each
  // embedded/data-uri badge, keyed by its synthetic `file://` sentinel uri, and
  // is reconciled into the outgoing blocks by `buildDraft`.
  const [attachments, setAttachmentState] = useState<InputAttachment[]>([])
  const attachmentsRef = useRef<InputAttachment[]>([])
  const [sendPending, setSendPending] = useState(false)
  const sendPendingRef = useRef(false)
  const resolvedPlaceholder = sendPending
    ? t("conversationStarting")
    : (placeholder ?? t("askAnything"))
  const showPlaceholderActivity = sendPending || animatePlaceholder
  const composerMutationVersionRef = useRef(0)
  const programmaticResetRef = useRef(false)
  const setAttachments = useCallback(
    (update: SetStateAction<InputAttachment[]>) => {
      const current = attachmentsRef.current
      const next = typeof update === "function" ? update(current) : update
      releaseUnusedImagePreviews(current, next)
      attachmentsRef.current = next
      if (!programmaticResetRef.current) {
        composerMutationVersionRef.current += 1
      }
      if (effectiveDraftStorageKey && !isEditingQueueItem) {
        setLiveMessageInputDraftPresence(
          effectiveDraftStorageKey,
          next.length > 0 || !(editorRef.current?.isEmpty() ?? true)
        )
      }
      const editorDoc = editorRef.current?.getJSON()
      onEphemeralDraftChange?.(
        next.length > 0 ||
          (editorDoc != null && hasEmbeddedReference(editorDoc))
      )
      setAttachmentState(next)
    },
    [effectiveDraftStorageKey, isEditingQueueItem, onEphemeralDraftChange]
  )
  const embeddedPayloadsRef = useRef<Map<string, PromptInputBlock>>(new Map())
  const [isDragActive, setIsDragActive] = useState(false)
  // Collapsed (narrow) selectors live in a controlled Popover holding a
  // master–detail panel (`SessionSelectorsPanel`). It's controlled so a value
  // pick closes it explicitly — matching the prior cog menu, which also closed
  // on every selection.
  const [collapsedSelectorsOpen, setCollapsedSelectorsOpen] = useState(false)
  const collapsedSelectorsGuard = useScrollbarSafeDismiss()
  const [quickMessages, setQuickMessages] = useState<QuickMessage[]>([])
  const [quickMessagesLoading, setQuickMessagesLoading] = useState(false)
  const [skillsMenuScanning, setSkillsMenuScanning] = useState(false)
  const [skillsMenuScanFailed, setSkillsMenuScanFailed] = useState(false)
  // Whether the async Clipboard read API is usable here. It's absent in
  // non-secure web deployments served over HTTP/LAN (see installClipboardFallback
  // in lib/utils, which only shims writeText), so the composer's custom
  // right-click "Paste" can't work there. When false we keep the radix context
  // menu disabled and let the browser's native menu through — its Paste still
  // works over the editable text. Resolved on the client after mount so SSR and
  // the first client render agree (no hydration mismatch on the trigger).
  const [clipboardReadSupported, setClipboardReadSupported] = useState(false)
  // Snapshotted when the custom right-click menu opens: whether the editor holds
  // a non-empty selection, which gates the Cut/Copy items. Read from the editor's
  // ProseMirror state (not the DOM Selection) so it stays correct after the radix
  // menu takes focus.
  const [contextSelectionActive, setContextSelectionActive] = useState(false)
  const [previewAttachmentId, setPreviewAttachmentId] = useState<string | null>(
    null
  )
  const containerRef = useRef<HTMLDivElement>(null)
  const lastDomDropAtRef = useRef(0)
  const disabledRef = useRef(disabled)
  const isPromptingRef = useRef(isPrompting)
  const sendGenerationRef = useRef(0)
  const hydratedRef = useRef(false)
  // Tracks the last queue-item id hydrated, so a re-edit of the *same* item
  // doesn't clobber the user's in-progress changes — keyed on id, not display
  // text (two attachment-only items share the text "Attached 1 attachment").
  const prevEditingItemIdRef = useRef<string | null>(null)
  const dragActiveRef = useRef(false)
  // Bridge so the early `onChange` handler can call the editor-driven slash
  // detection that is defined further down (after the slash state).
  const detectSlashTriggerRef = useRef<(() => void) | null>(null)
  useEffect(() => {
    if (isActive && !disabled && !isPrompting) {
      requestAnimationFrame(() => {
        editorRef.current?.focus()
      })
    }
  }, [isActive, disabled, isPrompting])

  useEffect(() => {
    disabledRef.current = disabled
  }, [disabled])

  useEffect(() => {
    const wasPrompting = isPromptingRef.current
    isPromptingRef.current = isPrompting
    if (wasPrompting && !isPrompting) {
      sendGenerationRef.current += 1
      setRunningTask(null)
    }
  }, [isPrompting])

  useEffect(() => {
    // navigator.clipboard is undefined at runtime in non-secure contexts even
    // though the DOM types claim it is always present, so guard with typeof.
    setClipboardReadSupported(
      typeof navigator !== "undefined" &&
        typeof navigator.clipboard?.readText === "function"
    )
  }, [])

  // Localized group headings + panel chrome for the `@` mention panel.
  const referenceGroupLabels = useMemo<ReferenceGroupLabels>(
    () => ({
      file: t("mentionGroupFile"),
      agent: t("mentionGroupAgent"),
      session: t("mentionGroupSession"),
      skill: t("mentionGroupSkill"),
    }),
    [t]
  )
  const mentionUiLabels = useMemo<MentionUiLabels>(
    () => ({
      empty: t("mentionEmpty"),
      loading: t("mentionLoading"),
      listbox: t("mentionListLabel"),
      more: t("mentionMore"),
      count: (count: number) => t("mentionCount", { count }),
    }),
    [t]
  )

  // Live data sources for the unified `@` mention panel. Pre-warmed only while
  // this composer is the active one (`enabled`). Referentially stable.
  const referenceSearch = useReferenceSearch({
    defaultPath: defaultPath ?? null,
    enabled: isActive,
    labels: referenceGroupLabels,
  })

  // Debounced v2 draft persistence. We snapshot the Tiptap *document* (JSON, not
  // Markdown) ~300ms after the last change so inline reference badges survive a
  // reload — a Markdown round-trip would downgrade them to plain links.
  const draftSaveTimerRef = useRef<number | null>(null)
  const pendingDraftDocRef = useRef<JSONContent | null>(null)
  const cancelPendingDraftSave = useCallback(() => {
    if (draftSaveTimerRef.current != null && typeof window !== "undefined") {
      window.clearTimeout(draftSaveTimerRef.current)
    }
    draftSaveTimerRef.current = null
    pendingDraftDocRef.current = null
  }, [])
  const persistPendingDraft = useCallback(() => {
    if (!effectiveDraftStorageKey || isEditingQueueItem) return
    const doc = pendingDraftDocRef.current
    pendingDraftDocRef.current = null
    if (doc) saveMessageInputDraftV2(effectiveDraftStorageKey, doc)
    else clearMessageInputDraftV2(effectiveDraftStorageKey)
    setLiveMessageInputDraftPresence(
      effectiveDraftStorageKey,
      doc != null || attachmentsRef.current.length > 0
    )
  }, [effectiveDraftStorageKey, isEditingQueueItem])
  const scheduleDraftSave = useCallback(() => {
    if (typeof window === "undefined") return
    if (!effectiveDraftStorageKey || isEditingQueueItem) return
    const ed = editorRef.current
    const editorDoc = ed?.getJSON()
    pendingDraftDocRef.current =
      !ed || ed.isEmpty() || !editorDoc
        ? null
        : stripEmbeddedReferences(editorDoc)
    onEphemeralDraftChange?.(
      attachmentsRef.current.length > 0 ||
        (editorDoc != null && hasEmbeddedReference(editorDoc))
    )
    setLiveMessageInputDraftPresence(
      effectiveDraftStorageKey,
      pendingDraftDocRef.current != null || attachmentsRef.current.length > 0
    )
    if (draftSaveTimerRef.current != null) {
      window.clearTimeout(draftSaveTimerRef.current)
    }
    draftSaveTimerRef.current = window.setTimeout(() => {
      draftSaveTimerRef.current = null
      persistPendingDraft()
    }, 300)
  }, [
    effectiveDraftStorageKey,
    isEditingQueueItem,
    onEphemeralDraftChange,
    persistPendingDraft,
  ])

  useEffect(() => {
    return () => releaseUnusedImagePreviews(attachmentsRef.current, [])
  }, [])

  useEffect(() => {
    return () => {
      if (draftSaveTimerRef.current != null && typeof window !== "undefined") {
        window.clearTimeout(draftSaveTimerRef.current)
        draftSaveTimerRef.current = null
        persistPendingDraft()
      }
      if (effectiveDraftStorageKey) {
        setLiveMessageInputDraftPresence(effectiveDraftStorageKey, false)
      }
      onEphemeralDraftChange?.(false)
    }
  }, [effectiveDraftStorageKey, onEphemeralDraftChange, persistPendingDraft])

  const uploadRestoredImages = useCallback(
    (uploads: RestoredImageUpload[]) => {
      for (const { attachment, file } of uploads) {
        void uploadChatImage(file, {
          ...chatImageStorage,
          mimeType: attachment.mimeType,
        })
          .then((prepared) => {
            setAttachments((current) =>
              updateImageAttachment(current, attachment.id, (item) => ({
                ...item,
                data: "",
                uri: prepared.url,
                localPath: prepared.localPath,
                name: prepared.name,
                mimeType: prepared.mimeType,
                sourceMimeType: prepared.mimeType,
                staging: undefined,
              }))
            )
          })
          .catch((error) => {
            console.error("[MessageInput] restored image upload failed", {
              name: attachment.name,
              error,
            })
            setAttachments((current) =>
              updateImageAttachment(current, attachment.id, (item) => ({
                ...item,
                staging: {
                  status: "failed",
                  source: { kind: "browser-file", file },
                },
              }))
            )
            toast.error(
              tAttach("attachUploadFailed", { names: attachment.name })
            )
          })
      }
    },
    [chatImageStorage, setAttachments, tAttach]
  )

  // Replay a sent `PromptInputBlock[]` (a queued message being re-edited) into
  // the editor: prose + file badges inline, images into `attachments`, and any
  // embedded/data-uri resources re-inlined as sentinel badges with their
  // bytes-bearing blocks re-registered in the payload map.
  const hydrateFromBlocks = useCallback(
    (editor: Editor, blocks: PromptInputBlock[]) => {
      embeddedPayloadsRef.current.clear()
      const restored = restoreBlocksIntoEditor(editor, blocks)
      const { images, uploads } = prepareRestoredImages(restored)
      setAttachments(images)
      uploadRestoredImages(uploads)
      insertRestoredResources(editor, restored, embeddedPayloadsRef.current)
    },
    [setAttachments, uploadRestoredImages]
  )

  // One-time hydration once the editor is ready: a queue-edit payload, else a v2
  // draft document (or a legacy v1 Markdown draft migrated forward). Guarded so
  // it never re-runs and clobbers later user edits.
  useEffect(() => {
    if (!composerReady || hydratedRef.current) return
    hydratedRef.current = true
    if (!editorRef.current) return
    // Bookkeeping stays synchronous so the sibling re-hydrate effect below sees
    // the claimed item and doesn't double-hydrate; only the editor mutation is
    // deferred to the next frame. Restoring a draft/queue payload that contains
    // a reference badge inserts a React NodeView, which @tiptap/react renders
    // with a synchronous flushSync() — running that here in the effect body
    // trips React's "flushSync from inside a lifecycle method" warning.
    if (
      isEditingQueueItem &&
      (editingDraftBlocks != null || editingDraftText != null)
    ) {
      prevEditingItemIdRef.current = editingItemId ?? null
    }
    const raf = requestAnimationFrame(() => {
      const ed = editorRef.current
      if (!ed) {
        setComposerHydrated(true)
        return
      }
      programmaticResetRef.current = true
      try {
        if (
          isEditingQueueItem &&
          (editingDraftBlocks != null || editingDraftText != null)
        ) {
          const editor = ed.getEditor()
          if (editingDraftBlocks && editingDraftBlocks.length > 0 && editor) {
            // Full fidelity: restore inline badges + images from the blocks.
            hydrateFromBlocks(editor, editingDraftBlocks)
          } else if (editingDraftText != null) {
            ed.setText(editingDraftText)
          }
        } else if (effectiveDraftStorageKey) {
          const loaded = loadMessageInputDraftV2(effectiveDraftStorageKey)
          if (loaded?.kind === "doc") {
            ed.setDoc(loaded.doc)
          } else if (loaded?.kind === "legacyMarkdown") {
            ed.setText(loaded.markdown)
          }
        }
      } finally {
        programmaticResetRef.current = false
      }
      const editor = ed.getEditor()
      if (editor) normalizeDirectiveReferences(editor)
      setComposerEmpty(editor ? isComposerEmpty(editor) : true)
      setDraftTask(editor ? getTaskReference(editor) : null)
      setComposerHydrated(true)
    })
    return () => cancelAnimationFrame(raf)
  }, [
    composerReady,
    isEditingQueueItem,
    editingItemId,
    editingDraftText,
    editingDraftBlocks,
    effectiveDraftStorageKey,
    hydrateFromBlocks,
  ])

  // Re-hydrate when the user (re)edits a *different* queue item after the
  // initial mount hydration above. Keyed on the item id (not display text) so
  // switching between two items with identical text still reloads.
  useEffect(() => {
    if (
      isEditingQueueItem &&
      editingItemId != null &&
      editingItemId !== prevEditingItemIdRef.current
    ) {
      prevEditingItemIdRef.current = editingItemId
      // Same flushSync deferral as the hydration effect above: hydrateFromBlocks
      // can insert reference-badge NodeViews (synchronous @tiptap/react
      // flushSync). Mutation + focus run next frame, off the commit phase.
      const raf = requestAnimationFrame(() => {
        const editor = editorRef.current?.getEditor()
        if (editingDraftBlocks && editingDraftBlocks.length > 0 && editor) {
          hydrateFromBlocks(editor, editingDraftBlocks)
        } else if (editingDraftText != null) {
          editorRef.current?.setText(editingDraftText)
        }
        setComposerEmpty(editor ? isComposerEmpty(editor) : true)
        setDraftTask(editor ? getTaskReference(editor) : null)
        editorRef.current?.focus()
      })
      return () => cancelAnimationFrame(raf)
    } else if (!isEditingQueueItem) {
      prevEditingItemIdRef.current = null
    }
  }, [
    isEditingQueueItem,
    editingItemId,
    editingDraftText,
    editingDraftBlocks,
    hydrateFromBlocks,
  ])

  useEffect(() => {
    if (!injectContent || !composerReady) return
    const payload = injectContent
    // Defer the editor mutation to the next frame. Inserting the skill badge
    // creates a React NodeView, which @tiptap/react renders with a synchronous
    // flushSync(); doing that here in the effect body runs flushSync during
    // React's commit phase and trips the "flushSync was called from inside a
    // lifecycle method" warning. Scheduling it out of the commit phase is the
    // same rAF pattern the hydration effects above use. onInjectConsumed fires
    // inside the frame so the synchronous body never flips injectContent → null
    // and lets the cleanup cancel our own rAF before it runs.
    const raf = requestAnimationFrame(() => {
      const handle = editorRef.current
      if (handle) {
        if (payload.scenario) {
          handle.setScenarioTemplate(payload.text, payload.scenario.variables)
        } else {
          handle.setText(payload.text)
        }
        // Prepend the skill as the leading invocation badge, so the sent
        // message opens with `${prefix}${id}`.
        if (payload.skill) {
          const editor = handle.getEditor()
          if (editor) {
            applyExpertReference(editor, {
              refType: "skill",
              id: payload.skill.id,
              label: payload.skill.label,
              uri: null,
              meta: {
                invocationPrefix: skillPrefix,
                scope: "expert",
                marketSkillId: payload.skill.package?.id,
                marketSkillSlug: payload.skill.package?.slug,
                marketSkillVersion: payload.skill.package?.version,
              },
            })
          }
        }
        setComposerEmpty(false)
        handle.focus()
      }
      onInjectConsumed?.()
    })
    return () => cancelAnimationFrame(raf)
  }, [injectContent, composerReady, skillPrefix, onInjectConsumed])

  // Skill and expert badges capture the selected agent's invocation prefix at
  // insert time. Keep existing badges aligned when the user switches agents.
  useEffect(() => {
    if (!composerReady) return
    const raf = requestAnimationFrame(() => {
      const editor = editorRef.current?.getEditor()
      if (editor) restampSkillPrefixes(editor, skillPrefix)
    })
    return () => cancelAnimationFrame(raf)
  }, [skillPrefix, composerReady])

  const setDragActiveIfChanged = useCallback((next: boolean) => {
    if (dragActiveRef.current === next) return
    dragActiveRef.current = next
    setIsDragActive(next)
  }, [])

  const syncComposerEmpty = useCallback(() => {
    const ed = editorRef.current?.getEditor()
    setComposerEmpty(ed ? isComposerEmpty(ed) : true)
  }, [])

  const handleComposerChange = useCallback(() => {
    syncComposerEmpty()
    const editor = editorRef.current?.getEditor()
    setDraftTask(editor ? getTaskReference(editor) : null)
    if (programmaticResetRef.current) return
    composerMutationVersionRef.current += 1
    scheduleDraftSave()
    detectSlashTriggerRef.current?.()
  }, [syncComposerEmpty, scheduleDraftSave])

  const handleComposerReady = useCallback(() => {
    setComposerReady(true)
  }, [])

  const availableModes = useMemo(() => modes ?? [], [modes])
  const rawConfigOptions = useMemo(() => configOptions ?? [], [configOptions])
  const availableConfigOptions = useMemo(
    () =>
      rawConfigOptions.map((option) =>
        localizeSessionConfigOption(option, tSessionConfig)
      ),
    [rawConfigOptions, tSessionConfig]
  )
  const modelBehaviorOptions = useMemo(
    () => availableConfigOptions.filter(isModelBehaviorConfigOption),
    [availableConfigOptions]
  )
  const topLevelConfigOptions = useMemo(
    () =>
      availableConfigOptions.filter(
        (option) => !isModelBehaviorConfigOption(option)
      ),
    [availableConfigOptions]
  )
  const hasConfigOptions = topLevelConfigOptions.length > 0
  const hasModes = availableModes.length > 0

  const effectiveModeId = useMemo(() => {
    if (!hasModes) return null
    if (
      selectedModeId &&
      availableModes.some((mode) => mode.id === selectedModeId)
    ) {
      return selectedModeId
    }
    return availableModes[0]?.id ?? null
  }, [hasModes, selectedModeId, availableModes])
  const showModeSelector = hasModes && Boolean(effectiveModeId)
  const showModeLoading = modeLoading && !showModeSelector
  const showConfigLoading = configOptionsLoading && !hasConfigOptions
  const orderedSessionSelectors = useMemo(
    () => orderSessionSelectors(showModeSelector, topLevelConfigOptions),
    [showModeSelector, topLevelConfigOptions]
  )
  const orderedConfigOptions = useMemo(
    () =>
      orderedSessionSelectors.flatMap((selector) =>
        selector.kind === "config" ? [selector.option] : []
      ),
    [orderedSessionSelectors]
  )
  const hasAnySelector =
    showConfigLoading || hasConfigOptions || showModeLoading || showModeSelector
  const hasInlineSelectors = hasConfigOptions || showModeSelector
  const hasFolderBranchPicker =
    useConversationFolderBranchPickerVisible(attachmentTabId)
  const folderBranchPickerAttached = hasFolderBranchPicker
  const imageAttachments = useMemo(
    () =>
      attachments.filter(
        (attachment): attachment is ImageInputAttachment =>
          attachment.type === "image"
      ),
    [attachments]
  )
  const previewAttachment = useMemo(
    () =>
      previewAttachmentId
        ? (imageAttachments.find((a) => a.id === previewAttachmentId) ?? null)
        : null,
    [previewAttachmentId, imageAttachments]
  )
  const previewAttachmentSrc = previewAttachment
    ? imageAttachmentSrc(previewAttachment)
    : ""
  const previewAttachmentIndex = useMemo(
    () =>
      previewAttachmentId
        ? imageAttachments.findIndex((item) => item.id === previewAttachmentId)
        : -1,
    [previewAttachmentId, imageAttachments]
  )
  const hasAttachments = attachments.length > 0
  const hasSendableContent = !composerEmpty || hasAttachments
  const hasUnstagedImage = imageAttachments.some(
    (attachment) => attachment.staging !== undefined
  )

  // ── Slash command autocomplete ──
  //
  // The slash list shows the agent's own `availableCommands` verbatim —
  // experts are advertised as commands and now appear here alongside the
  // rest. Codex additionally gets a `$`-triggered skills list (experts are
  // symlinked skills, so they surface there) because its native command set
  // is very small.
  const [slashMenuOpen, setSlashMenuOpen] = useState(false)
  const [slashSelectedIndex, setSlashSelectedIndex] = useState(0)
  // The trigger char (`/` for agent commands, `$` for Codex skills) and the
  // typed filter token, both derived from the editor caret by
  // `detectSlashTrigger` rather than from a raw string offset.
  const [slashTriggerChar, setSlashTriggerChar] = useState<"/" | "$" | null>(
    null
  )
  const [slashFilter, setSlashFilter] = useState("")
  const slashCommands = useMemo(
    () =>
      (availableCommands ?? []).filter((command) =>
        isVisibleExpertId(command.name.replace(/^\/+/, "").toLowerCase())
      ),
    [availableCommands]
  )
  const filteredSlashCommands = useMemo(() => {
    if (!slashMenuOpen || slashCommands.length === 0) return []
    if (slashTriggerChar !== "/") return []
    const filter = slashFilter.toLowerCase()
    return slashCommands.filter((cmd) =>
      cmd.name.toLowerCase().includes(filter)
    )
  }, [slashMenuOpen, slashCommands, slashTriggerChar, slashFilter])
  const filteredSlashSkills = useMemo(() => {
    // Skills autocomplete is Codex-only and triggered by `$`.
    if (agentType !== "codex") return []
    if (!slashMenuOpen || visibleAvailableSkills.length === 0) return []
    if (slashTriggerChar !== "$") return []
    const filter = slashFilter.toLowerCase()
    if (!filter) return visibleAvailableSkills
    const nameMatches: typeof visibleAvailableSkills = []
    const idOnlyMatches: typeof visibleAvailableSkills = []
    for (const skill of visibleAvailableSkills) {
      if (skill.name.toLowerCase().includes(filter)) {
        nameMatches.push(skill)
      } else if (skill.id.toLowerCase().includes(filter)) {
        idOnlyMatches.push(skill)
      }
    }
    return [...nameMatches, ...idOnlyMatches]
  }, [
    slashMenuOpen,
    visibleAvailableSkills,
    agentType,
    slashTriggerChar,
    slashFilter,
  ])
  const slashAutocompleteCount =
    filteredSlashCommands.length + filteredSlashSkills.length

  // Keep the highlighted row inside the current result window. As the user
  // types and the filter narrows, the previously-highlighted index can point
  // past the end of the merged list (commands + experts), which would make
  // Enter/Tab a silent no-op. Clamp back to the last available row whenever
  // the count changes.
  useEffect(() => {
    if (
      slashAutocompleteCount > 0 &&
      slashSelectedIndex >= slashAutocompleteCount
    ) {
      setSlashSelectedIndex(slashAutocompleteCount - 1)
    }
  }, [slashAutocompleteCount, slashSelectedIndex])

  // Keep the highlighted row visible inside the popup when keyboard navigation
  // pushes it past the scroll viewport. Without this the cursor silently runs
  // off the rendered area when the filtered list overflows `max-h`.
  const slashMenuListRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!slashMenuOpen) return
    const container = slashMenuListRef.current
    if (!container) return
    const el = container.children[slashSelectedIndex] as HTMLElement | undefined
    if (!el) return
    const elTop = el.offsetTop
    const elBottom = elTop + el.offsetHeight
    const viewTop = container.scrollTop
    const viewBottom = viewTop + container.clientHeight
    if (elTop < viewTop) {
      container.scrollTop = elTop
    } else if (elBottom > viewBottom) {
      container.scrollTop = elBottom - container.clientHeight
    }
  }, [slashMenuOpen, slashSelectedIndex, slashAutocompleteCount])

  // ── Editor-driven `/` (commands) and `$` (Codex skills) trigger detection ──
  // The `@` mention panel is now owned by RichComposer; this only handles the
  // runtime-command menus. We inspect the text before the collapsed caret in the
  // current block: a `/` (any agent) or `$` (Codex) at the start or right after
  // whitespace, and not inside inline code / a code block, opens the menu.
  const detectSlashTrigger = useCallback(() => {
    const editor = editorRef.current?.getEditor()
    const hasSlashSource =
      slashCommands.length > 0 || visibleAvailableSkills.length > 0
    const close = () => {
      setSlashMenuOpen(false)
      setSlashTriggerChar(null)
    }
    if (!editor || !hasSlashSource) return close()
    const { selection } = editor.state
    if (!selection.empty) return close()
    if (editor.isActive("code") || editor.isActive("codeBlock")) return close()
    const { $from } = selection
    const before = $from.parent.textBetween(
      0,
      $from.parentOffset,
      undefined,
      " "
    )
    const regex =
      agentType === "codex" ? /(^|\s)([/$])(\S*)$/ : /(^|\s)(\/)(\S*)$/
    const match = before.match(regex)
    if (!match) return close()
    setSlashTriggerChar(match[2] as "/" | "$")
    setSlashFilter(match[3])
    setSlashSelectedIndex(0)
    setSlashMenuOpen(true)
  }, [slashCommands.length, visibleAvailableSkills.length, agentType])

  useEffect(() => {
    detectSlashTriggerRef.current = detectSlashTrigger
  }, [detectSlashTrigger])

  // Insert one inline file reference badge per item, matching `@`-file mentions.
  // A genuine `file://` item uses its uri directly (deduped against the document);
  // an item carrying a `realBlock` (embedded bytes / `data:` link) gets an inert
  // `iyw-claw://embedded/…` display uri and its block is stashed in
  // `embeddedPayloadsRef` for send-time reconciliation. Badges append at the doc
  // end by default; pass `atCaret` to drop them at the composer's current caret
  // (`focus()` keeps the retained selection even while the input is blurred —
  // e.g. focus sits in the file editor), so "add to chat" lands a reference
  // where the user left off instead of always at the end.
  const insertFileReferences = useCallback(
    (
      items: Array<{
        name: string
        uri?: string
        realBlock?: PromptInputBlock
        fileKind?: "file" | "dir"
      }>,
      opts: { atCaret?: boolean } = {}
    ) => {
      if (items.length === 0) return
      const editor = editorRef.current?.getEditor()
      if (!editor) return
      const seen = new Set<string>()
      let chain = opts.atCaret
        ? editor.chain().focus()
        : editor.chain().focus("end")
      let inserted = 0
      for (const item of items) {
        let refUri: string
        if (item.realBlock) {
          refUri = buildEmbeddedReferenceUri()
          embeddedPayloadsRef.current.set(refUri, item.realBlock)
        } else {
          if (!item.uri) continue
          refUri = item.uri
          if (seen.has(refUri) || editorHasFileReference(editor, refUri))
            continue
          seen.add(refUri)
        }
        chain = chain
          .insertReference({
            refType: "file",
            id: refUri,
            label: item.name,
            uri: refUri,
            meta: { fileKind: item.fileKind ?? "file" },
          })
          .insertContent(" ")
        inserted++
      }
      if (inserted > 0) chain.run()
    },
    []
  )

  const appendResourceLinks = useCallback(
    (
      links: Array<{
        uri: string
        name: string
        mimeType: string | null
        dedupeKey: string
      }>,
      opts: { atCaret?: boolean } = {}
    ) => {
      // `file://` links the agent can read directly become inline file badges
      // (uri used as-is); a non-fetchable `data:` link keeps its real block out
      // of band behind a sentinel badge.
      insertFileReferences(
        links
          .filter((link) => link.uri)
          .map((link) =>
            link.uri.toLowerCase().startsWith("file://")
              ? { name: link.name, uri: link.uri }
              : {
                  name: link.name,
                  realBlock: {
                    type: "resource_link" as const,
                    uri: link.uri,
                    name: link.name,
                    mime_type: link.mimeType,
                    description: null,
                  },
                }
          ),
        opts
      )
    },
    [insertFileReferences]
  )

  const appendResourceAttachments = useCallback(
    (paths: string[], opts: { atCaret?: boolean } = {}) => {
      const normalized = paths
        .filter(
          (path): path is string => typeof path === "string" && path.length > 0
        )
        .map((path) => {
          const uri = buildFileUri(path)
          return {
            uri,
            name: fileNameFromPath(path),
            mimeType: mimeTypeFromPath(path),
            dedupeKey: uri,
          }
        })
      appendResourceLinks(normalized, opts)
    },
    [appendResourceLinks]
  )

  const appendImageAttachment = useCallback(
    (
      uri: string | null,
      name: string,
      mimeType: string,
      options: {
        id?: string
        data?: string
        localPath?: string | null
        previewUrl?: string
        sourceMimeType?: string
        staging?: ImageAttachmentStaging
      } = {}
    ) => {
      const data = options.data ?? ""
      const size = bytesFromBase64(data)
      if (
        !isPublicImageUrl(uri) &&
        size === 0 &&
        !options.previewUrl &&
        !options.staging
      ) {
        throw new Error("Image URL is invalid")
      }
      if (data && size > IMAGE_ATTACHMENT_MAX_BYTES) {
        throw new Error(
          `Image exceeds the ${IMAGE_ATTACHMENT_MAX_BYTES / (1024 * 1024)}MB attachment limit`
        )
      }
      const id = options.id ?? randomUUID()
      setAttachments((current) => [
        ...(uri &&
        current.some(
          (attachment) => attachment.type === "image" && attachment.uri === uri
        )
          ? current.filter(
              (attachment) =>
                attachment.type !== "image" || attachment.uri !== uri
            )
          : current),
        {
          id,
          type: "image",
          data,
          uri,
          localPath: options.localPath,
          name: name || "image",
          mimeType,
          previewUrl: options.previewUrl,
          sourceMimeType: options.sourceMimeType,
          staging: options.staging,
        },
      ])
      return id
    },
    [setAttachments]
  )

  const applyStagingImagePreview = useCallback(
    (id: string, previewUrl: string) => {
      const attachment = attachmentsRef.current.find(
        (item): item is ImageInputAttachment =>
          item.type === "image" && item.id === id
      )
      if (!attachment?.staging || attachment.previewUrl) {
        URL.revokeObjectURL(previewUrl)
        return
      }
      setAttachments((current) =>
        updateImageAttachment(current, id, (item) =>
          item.staging && !item.previewUrl ? { ...item, previewUrl } : item
        )
      )
    },
    [setAttachments]
  )

  const appendImageFile = useCallback(
    async (file: File): Promise<boolean> => {
      const mimeType = imageMimeTypeForFile(file)
      if (!mimeType) return false
      if (file.size > CHAT_IMAGE_SOURCE_MAX_BYTES) {
        toast.error(
          tAttach("attachImageTooLarge", {
            name: file.name,
            limit: CHAT_IMAGE_SOURCE_MAX_BYTES / (1024 * 1024),
            size: (file.size / (1024 * 1024)).toFixed(1),
          })
        )
        return true
      }
      const previewUrl = URL.createObjectURL(file)
      const source: ImageAttachmentStaging["source"] = {
        kind: "browser-file",
        file,
      }
      const id = appendImageAttachment(null, file.name, mimeType, {
        previewUrl,
        sourceMimeType: mimeType,
        staging: { status: "uploading", source },
      })
      try {
        const prepared = await uploadChatImage(file, {
          ...chatImageStorage,
          mimeType,
        })
        setAttachments((current) =>
          applyPreparedImage(current, id, prepared, mimeType)
        )
      } catch (error) {
        console.error("[MessageInput] image file staging failed", {
          name: file.name,
          mimeType,
          size: file.size,
          error,
        })
        setAttachments((current) =>
          updateImageAttachment(current, id, (item) => ({
            ...item,
            staging: { status: "failed", source },
          }))
        )
        toast.error(tAttach("attachUploadFailed", { names: file.name }))
      }
      return true
    },
    [appendImageAttachment, chatImageStorage, setAttachments, tAttach]
  )

  const appendImagePath = useCallback(
    async (
      path: string,
      source: "local" | "workspace" = "local",
      opts: { sourceMimeType?: string } = {}
    ) => {
      const mimeType = imageMimeTypeFromPath(path)
      if (!mimeType) return false
      const name = fileNameFromPath(path)
      const stagingSource: ImageAttachmentStaging["source"] = {
        kind: "local-path",
        path,
        source,
      }
      const id = appendImageAttachment(null, name, mimeType, {
        sourceMimeType: opts.sourceMimeType ?? mimeType,
        staging: { status: "uploading", source: stagingSource },
      })
      void createImagePathPreview(path, source, name, mimeType).then(
        (previewUrl) => {
          if (previewUrl) applyStagingImagePreview(id, previewUrl)
        }
      )
      try {
        const prepared = await prepareChatImagePath(
          path,
          source,
          chatImageStorage
        )
        setAttachments((current) =>
          applyPreparedImage(
            current,
            id,
            prepared,
            opts.sourceMimeType ?? mimeType
          )
        )
      } catch (error) {
        setAttachments((current) =>
          updateImageAttachment(current, id, (item) => ({
            ...item,
            staging: { status: "failed", source: stagingSource },
          }))
        )
        const tooLarge = imageTooLargeDetails(error, name)
        if (tooLarge) {
          toast.error(tAttach("attachImageTooLarge", tooLarge))
          return true
        }
        console.error("[MessageInput] image path read failed", {
          name,
          mimeType,
          error,
        })
        toast.error(tAttach("attachImageReadFailed", { name }))
        return true
      }
      return true
    },
    [
      appendImageAttachment,
      applyStagingImagePreview,
      chatImageStorage,
      setAttachments,
      tAttach,
    ]
  )

  const handleComposerReferenceSelect = useCallback(
    (reference: ReferenceAttrs) => {
      if (reference.refType !== "file" || !reference.uri) return false
      const path = fileUriToPath(reference.uri)
      if (!path || !imageMimeTypeFromPath(path)) return false
      const source = showNativePaperclip ? "local" : "workspace"
      void appendImagePath(path, source).then((isImage) => {
        if (!isImage) editorRef.current?.insertReference(reference)
      })
      return true
    },
    [appendImagePath, showNativePaperclip]
  )

  const appendRemoteLocalImagePath = useCallback(
    async (path: string): Promise<boolean> => {
      const sourceMimeType = imageMimeTypeFromPath(path)
      if (!sourceMimeType) return false
      const name = fileNameFromPath(path)
      const source: ImageAttachmentStaging["source"] = {
        kind: "local-path",
        path,
        source: "remote-local",
      }
      const id = appendImageAttachment(null, name, sourceMimeType, {
        sourceMimeType,
        staging: { status: "uploading", source },
      })
      void createImagePathPreview(
        path,
        "remote-local",
        name,
        sourceMimeType
      ).then((previewUrl) => {
        if (previewUrl) applyStagingImagePreview(id, previewUrl)
      })
      try {
        const staged = await uploadLocalChatImagePathToRemote(
          path,
          chatImageStorage
        )
        setAttachments((current) =>
          applyPreparedImage(current, id, staged, sourceMimeType)
        )
        return true
      } catch (error) {
        setAttachments((current) =>
          updateImageAttachment(current, id, (item) => ({
            ...item,
            staging: { status: "failed", source },
          }))
        )
        const tooLarge = imageTooLargeDetails(error, name)
        if (tooLarge) {
          toast.error(tAttach("attachImageTooLarge", tooLarge))
          return true
        }
        console.error("[MessageInput] remote image staging failed", {
          name,
          error,
        })
        toast.error(tAttach("attachUploadFailed", { names: name }))
        return true
      }
    },
    [
      appendImageAttachment,
      applyStagingImagePreview,
      chatImageStorage,
      setAttachments,
      tAttach,
    ]
  )

  // Attach a single file as a ranged badge (`foo.ts:10-25`), used by the file
  // editor's "add selection to chat". The line span is encoded into both the
  // label and the uri fragment (`file://…#L10-25`), so distinct selections of
  // the same file stay distinct (the uri is the dedupe key in
  // `insertFileReferences`) and the range rides along to the agent in the
  // serialized `[label](uri)` link.
  const appendFileRangeAttachment = useCallback(
    (
      path: string,
      range: { start: number; end: number },
      opts: { atCaret?: boolean } = {}
    ) => {
      if (!path) return
      insertFileReferences(
        [
          {
            name: formatFileRangeLabel(fileNameFromPath(path), range),
            uri: buildFileUriWithRange(path, range),
          },
        ],
        opts
      )
    },
    [insertFileReferences]
  )

  // Shared upload pool used by the menu's "Upload local file" button,
  // browser drag-drop in web mode, paste in web mode, and the fallback
  // path of `appendFilesAsResources` for remote-desktop. Local desktop staging
  // uses the same 100 MiB ceiling in every transport; path-backed local files
  // above that threshold bypass this pool earlier and remain direct links.
  const uploadAndAppendFiles = useCallback(
    async (files: File[]) => {
      if (files.length === 0) return
      const oversized = files.filter((f) => f.size > UPLOAD_MAX_BYTES)
      const accepted = files.filter((f) => f.size <= UPLOAD_MAX_BYTES)
      const limitMb = Math.round(UPLOAD_MAX_BYTES / (1024 * 1024))
      if (oversized.length > 0) {
        toast.error(
          tAttach("attachUploadTooLarge", {
            limit: limitMb,
            names: oversized.map((f) => f.name).join(", "),
          })
        )
      }
      if (accepted.length === 0) return

      // Concurrent uploads — one failure doesn't block the rest. Cap at 3:
      // small enough to keep server load predictable, large enough to feel
      // responsive for a handful of files.
      const uploaded: string[] = []
      const failed: Array<{ name: string; reason: unknown }> = []
      const quotaRejected: string[] = []
      const CONCURRENCY = 3
      let cursor = 0
      const workers = Array.from(
        { length: Math.min(CONCURRENCY, accepted.length) },
        async () => {
          while (cursor < accepted.length) {
            const idx = cursor++
            const file = accepted[idx]
            try {
              const r = await uploadAttachment(
                file,
                attachmentTabId ?? null,
                stageAttachmentsInWorkingDir ? (defaultPath ?? null) : null
              )
              uploaded.push(r.path)
            } catch (error) {
              if (isEmptyAttachmentError(error)) {
                // Empty files are explicitly dropped on the floor — log
                // and move on without a user-facing error toast.
                console.warn(
                  `[MessageInput] skipping empty attachment: ${file.name}`
                )
                continue
              }
              const appError = extractAppCommandError(error)
              if (appError?.i18n_key === UPLOAD_I18N_KEY_QUOTA_EXCEEDED) {
                quotaRejected.push(file.name)
                continue
              }
              failed.push({ name: file.name, reason: error })
            }
          }
        }
      )
      await Promise.all(workers)

      if (quotaRejected.length > 0) {
        toast.error(
          tAttach("attachUploadQuotaExceeded", {
            names: quotaRejected.join(", "),
          })
        )
      }
      if (failed.length > 0) {
        for (const f of failed) {
          console.error(
            `[MessageInput] upload attachment failed (${f.name}):`,
            f.reason
          )
        }
        toast.error(
          tAttach("attachUploadFailed", {
            names: failed.map((r) => r.name).join(", "),
          })
        )
      }
      if (uploaded.length > 0) {
        appendResourceAttachments(uploaded)
      }
    },
    [
      appendResourceAttachments,
      attachmentTabId,
      defaultPath,
      stageAttachmentsInWorkingDir,
      tAttach,
    ]
  )

  // Images are converted to ACP image blocks at this boundary. Ordinary files
  // retain the existing path/upload behavior so agents can inspect them with
  // their normal file tools.
  const appendFilesAsResources = useCallback(
    async (files: File[]) => {
      if (files.length === 0) return
      const localPaths: string[] = []
      const uploadCandidates: File[] = []

      const classified = await Promise.all(
        files.map(async (file) => {
          const path = getFilePath(file)
          if (path && showNativePaperclip) {
            return (await appendImagePath(path))
              ? null
              : ({ kind: "local-path", path } as const)
          }
          if (path && getActiveRemoteConnectionId() !== null) {
            if (await appendRemoteLocalImagePath(path)) return null
          }
          return (await appendImageFile(file))
            ? null
            : ({ kind: "upload", file } as const)
        })
      )
      for (const item of classified) {
        if (item?.kind === "local-path") localPaths.push(item.path)
        if (item?.kind === "upload") uploadCandidates.push(item.file)
      }

      if (localPaths.length > 0) {
        const prepared = await preparePickedAttachmentPaths(localPaths, {
          stageInChatDirectory: stageAttachmentsInWorkingDir,
          chatDirectory: defaultPath,
          stage: stageLocalChatAttachment,
        })
        appendResourceAttachments(prepared)
      }
      if (uploadCandidates.length > 0) {
        await uploadAndAppendFiles(uploadCandidates)
      }
    },
    [
      appendResourceAttachments,
      appendImageFile,
      appendImagePath,
      appendRemoteLocalImagePath,
      defaultPath,
      showNativePaperclip,
      stageAttachmentsInWorkingDir,
      uploadAndAppendFiles,
    ]
  )

  const appendPathsFromDrop = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return
      const normalized = paths.filter(
        (path): path is string => typeof path === "string" && path.length > 0
      )
      if (normalized.length === 0) return
      const prepared = await preparePickedAttachmentPaths(normalized, {
        stageInChatDirectory: stageAttachmentsInWorkingDir,
        chatDirectory: defaultPath,
        stage: stageLocalChatAttachment,
      })
      appendResourceAttachments(prepared)
    },
    [appendResourceAttachments, defaultPath, stageAttachmentsInWorkingDir]
  )

  const appendPathsFromDropRef = useRef(appendPathsFromDrop)
  useEffect(() => {
    appendPathsFromDropRef.current = appendPathsFromDrop
  }, [appendPathsFromDrop])

  // Remote-workspace counterpart of `appendPathsFromDrop`. Reads each
  // local path through Rust, ships the bytes via the upload proxy, then
  // appends the resulting server-side paths as ResourceLinks. Failures
  // (oversize, ENOENT, network) are reported in a single aggregated toast
  // matching `uploadAndAppendFiles`.
  const uploadPathsToRemote = useCallback(
    async (paths: string[]) => {
      const normalized = paths.filter(
        (p): p is string => typeof p === "string" && p.length > 0
      )
      if (normalized.length === 0) return

      const limitMb = Math.round(UPLOAD_MAX_BYTES / (1024 * 1024))
      const succeeded: string[] = []
      const failed: Array<{ name: string; reason: unknown }> = []
      const oversize: string[] = []
      const directories: string[] = []
      const quotaRejected: string[] = []

      const CONCURRENCY = 3
      let cursor = 0
      const workers = Array.from(
        { length: Math.min(CONCURRENCY, normalized.length) },
        async () => {
          while (cursor < normalized.length) {
            const idx = cursor++
            const path = normalized[idx]
            const name = path.split(/[/\\]/).pop() || path
            try {
              const r = await uploadLocalPathToRemote(
                path,
                attachmentTabId ?? null
              )
              succeeded.push(r.path)
            } catch (error) {
              if (isEmptyAttachmentError(error)) {
                console.warn(
                  `[MessageInput] skipping empty remote-drop attachment: ${name}`
                )
                continue
              }
              // The Rust side tags structured upload errors with an
              // `i18n_key` (see `app_error::UPLOAD_I18N_KEY_*`); branch
              // on the key so each user-visible category lands in its own
              // toast instead of the generic "upload failed" bucket.
              // Falling back to the bare message would couple us to the
              // exact English phrasing in `remote_proxy.rs`.
              const appError = extractAppCommandError(error)
              const i18nKey = appError?.i18n_key ?? null
              if (i18nKey === UPLOAD_I18N_KEY_TOO_LARGE) {
                oversize.push(name)
              } else if (i18nKey === UPLOAD_I18N_KEY_NOT_A_FILE) {
                // Dragging a directory or a special file (FIFO, device
                // node) lands here. The Rust guard short-circuits before
                // we even read bytes; surface a dedicated toast so the
                // user understands why nothing was attached.
                directories.push(name)
              } else if (i18nKey === UPLOAD_I18N_KEY_QUOTA_EXCEEDED) {
                quotaRejected.push(name)
              } else {
                failed.push({ name, reason: error })
              }
            }
          }
        }
      )
      await Promise.all(workers)

      if (oversize.length > 0) {
        toast.error(
          tAttach("attachUploadTooLarge", {
            limit: limitMb,
            names: oversize.join(", "),
          })
        )
      }
      if (directories.length > 0) {
        toast.error(
          tAttach("attachUploadNotAFile", {
            names: directories.join(", "),
          })
        )
      }
      if (quotaRejected.length > 0) {
        toast.error(
          tAttach("attachUploadQuotaExceeded", {
            names: quotaRejected.join(", "),
          })
        )
      }
      if (failed.length > 0) {
        for (const f of failed) {
          console.error(
            `[MessageInput] remote path upload failed (${f.name}):`,
            f.reason
          )
        }
        toast.error(
          tAttach("attachUploadFailed", {
            names: failed.map((f) => f.name).join(", "),
          })
        )
      }
      if (succeeded.length > 0) {
        appendResourceAttachments(succeeded)
      }
    },
    [appendResourceAttachments, attachmentTabId, tAttach]
  )

  const uploadPathsToRemoteRef = useRef(uploadPathsToRemote)
  useEffect(() => {
    uploadPathsToRemoteRef.current = uploadPathsToRemote
  }, [uploadPathsToRemote])

  const collectOrdinaryNativePaths = useCallback(
    async (paths: string[], remote: boolean) => {
      const classified = await Promise.all(
        paths.map(async (path) => ({
          path,
          handled: remote
            ? await appendRemoteLocalImagePath(path)
            : await appendImagePath(path),
        }))
      )
      return classified.filter((item) => !item.handled).map((item) => item.path)
    },
    [appendImagePath, appendRemoteLocalImagePath]
  )

  const appendNativePickedPaths = useCallback(
    async (paths: string[]) => {
      const remote = getActiveRemoteConnectionId() !== null
      const ordinary = await collectOrdinaryNativePaths(paths, remote)
      if (ordinary.length === 0) return
      if (remote) {
        await uploadPathsToRemoteRef.current(ordinary)
        return
      }
      const prepared = await preparePickedAttachmentPaths(ordinary, {
        stageInChatDirectory: stageAttachmentsInWorkingDir,
        chatDirectory: defaultPath,
        stage: stageLocalChatAttachment,
      })
      if (prepared.length > 0) appendResourceAttachments(prepared)
    },
    [
      appendResourceAttachments,
      collectOrdinaryNativePaths,
      defaultPath,
      stageAttachmentsInWorkingDir,
    ]
  )

  const appendFilesFromInput = useCallback(
    async (files: File[]) => {
      if (files.length === 0) return
      await appendFilesAsResources(files)
    },
    [appendFilesAsResources]
  )

  // Routed from RichComposer's `onPasteFiles`. Returns true when the paste was
  // consumed as an attachment (so the editor doesn't also insert it as text).
  const handlePasteFiles = useCallback(
    (event: ClipboardEvent): boolean => {
      if (disabled) return false
      // The context-menu "Paste" drives text through `view.pasteText`, which
      // runs this handler with a synthetic `new ClipboardEvent("paste")` whose
      // `clipboardData` is null. There's nothing to attach from it (and the
      // image fallback below would otherwise fire a stray async clipboard read),
      // so let the editor's own text paste proceed. Real pastes always carry a
      // (non-null) DataTransfer, so this never short-circuits a genuine paste.
      if (!event.clipboardData) return false
      const files = filesFromClipboard(event.clipboardData)
      if (files.length > 0) {
        void appendFilesFromInput(files).catch((error) => {
          console.error("[MessageInput] paste files failed:", error)
        })
        return true
      }

      // Linux/Tauri (WebKitGTK) fallback: screenshot tools (e.g. WeChat) write
      // the image to the clipboard in a form the synchronous DataTransfer API
      // can't read, so retry through the async Clipboard API. Only for a pure-
      // image clipboard — when text is present we let the default paste run
      // (mirroring `filesFromClipboard`) so copying a spreadsheet cell or rich
      // web content isn't hijacked into an image attachment. Kept synchronous
      // so `imageFilesFromClipboardApi` runs inside the paste user gesture.
      if (clipboardHasText(event.clipboardData)) return false
      void imageFilesFromClipboardApi()
        .then((imageFiles) => {
          if (imageFiles.length === 0) return
          return appendFilesFromInput(imageFiles)
        })
        .catch((error) => {
          console.error("[MessageInput] clipboard image paste failed:", error)
        })
      // The default paste of a textless clipboard is a no-op, so don't claim it.
      return false
    },
    [appendFilesFromInput, disabled]
  )

  useEffect(() => {
    if (!showModeSelector) return
    if (!effectiveModeId || !onModeChange) return
    if (effectiveModeId !== selectedModeId) {
      onModeChange(effectiveModeId)
    }
  }, [showModeSelector, effectiveModeId, selectedModeId, onModeChange])

  const handleModeSelect = useCallback(
    (modeId: string) => {
      onModeChange?.(modeId)
    },
    [onModeChange]
  )

  // Close the runtime-command menu and clear the trigger.
  const closeSlashMenu = useCallback(() => {
    setSlashMenuOpen(false)
    setSlashTriggerChar(null)
  }, [])

  // Replace the live `/`-or-`$` token immediately before the caret with
  // an inline reference badge (+ a trailing space unless one already follows),
  // then close the menu. Used by both the command (`/`) and Codex-skill (`$`)
  // selections — the badge serializes back to its literal `/cmd` / `$skill`
  // token on send (see invocation-reference / referenceToMarkdown).
  const replaceTriggerWithReference = useCallback(
    (ref: ReferenceAttrs) => {
      const editor = editorRef.current?.getEditor()
      if (!editor) return
      const { $from } = editor.state.selection
      const before = $from.parent.textBetween(
        0,
        $from.parentOffset,
        undefined,
        " "
      )
      const match = before.match(/(^|\s)([/$])(\S*)$/)
      const charAfter =
        $from.parentOffset < $from.parent.content.size
          ? $from.parent.textBetween(
              $from.parentOffset,
              $from.parentOffset + 1,
              undefined,
              " "
            )
          : ""
      const suffix = charAfter && /\s/.test(charAfter) ? "" : " "
      let chain = editor.chain().focus()
      if (match) {
        // Remove the live `/…` / `$…` token before the caret.
        const tokenLen = match[2].length + match[3].length
        chain = chain.deleteRange({ from: $from.pos - tokenLen, to: $from.pos })
      }
      if (isTaskReference(ref)) {
        chain.run()
        applyTaskReference(editor, ref)
      } else {
        chain = chain.insertReference(ref)
        if (suffix) chain = chain.insertContent(suffix)
        chain.run()
      }
      closeSlashMenu()
    },
    [closeSlashMenu]
  )

  const handleSlashSelect = useCallback(
    (cmd: AvailableCommandInfo) => {
      replaceTriggerWithReference(commandToReference(cmd))
    },
    [replaceTriggerWithReference]
  )

  // Codex uses `$<id>`, other agents `/<id>` — matching the trigger prefix.
  const handleSkillAutocompleteSelect = useCallback(
    (skill: AgentSkillItem) => {
      replaceTriggerWithReference(skillToReference(skill, skillPrefix))
    },
    [replaceTriggerWithReference, skillPrefix]
  )

  const handleTaskCommandSelect = useCallback((reference: ReferenceAttrs) => {
    const editor = editorRef.current?.getEditor()
    if (!editor) return
    applyTaskReference(editor, reference)
  }, [])

  const handleTaskRemove = useCallback(() => {
    const editor = editorRef.current?.getEditor()
    if (editor && getTaskReference(editor)) {
      clearTaskReference(editor)
      return
    }
    setRunningTask(null)
  }, [])

  const handleSkillMenuSelect = useCallback(
    (skill: AgentSkillItem) => {
      const editor = editorRef.current?.getEditor()
      if (!editor) return
      const reference = skillToReference(skill, skillPrefix)
      if (isTaskReference(reference)) {
        applyTaskReference(editor, reference)
        return
      }
      const { $from } = editor.state.selection
      const charBefore =
        $from.parentOffset > 0
          ? $from.parent.textBetween(
              $from.parentOffset - 1,
              $from.parentOffset,
              undefined,
              " "
            )
          : ""
      const needsSpace = charBefore !== "" && !/\s/.test(charBefore)
      let chain = editor.chain().focus()
      if (needsSpace) chain = chain.insertContent(" ")
      chain.insertReference(reference).insertContent(" ").run()
    },
    [skillPrefix]
  )

  // ── "+" menu skill shortcuts (experts / daily office) ──
  //
  // Surface the welcome-page skill families inside an active conversation. Each
  // item drops that skill's leading invocation badge into the composer. A skill
  // not linked to the current agent is "locked": clicking it surfaces a hint
  // (and opens its Skill Market detail) instead of injecting a badge the agent
  // can't act on — the same gating QuickActions applies, to avoid a wasted send.
  const expertsSorted = useMemo(
    () =>
      [...experts].sort(
        (a, b) =>
          (a.metadata.sort_order ?? 0) - (b.metadata.sort_order ?? 0) ||
          a.metadata.id.localeCompare(b.metadata.id)
      ),
    [experts]
  )
  const isSkillLocked = useCallback(
    (id: string) => !!agentType && skillStatusReady && !enabledIds.has(id),
    [agentType, skillStatusReady, enabledIds]
  )

  const notifySkillNotEnabled = useCallback(
    (skillLabel: string, skillId: string) => {
      const agentLabel = agentType ? getAgentDisplayName(agentType) : ""
      toast.warning(
        tQa("notEnabled.title", { skill: skillLabel, agent: agentLabel }),
        {
          description: tQa("notEnabled.description"),
          action: {
            label: tQa("notEnabled.action"),
            onClick: () => openSkillMarket(skillId),
          },
        }
      )
    },
    [agentType, openSkillMarket, tQa]
  )

  // Insert a skill shortcut: seed the template only into an *empty* composer
  // (never clobber an in-progress draft), then prepend the skill as the leading
  // invocation badge. Deferred to the next frame — inserting the badge mounts a
  // React NodeView rendered with a synchronous flushSync(), which warns if run
  // during React's commit phase (same pattern as the inject/hydration effects).
  const insertSkillShortcut = useCallback(
    (skill: { id: string; label: string }, template: string) => {
      requestAnimationFrame(() => {
        const handle = editorRef.current
        const editor = handle?.getEditor()
        if (!handle || !editor) return
        if (template && isComposerEmpty(editor)) {
          handle.setText(template)
        }
        applyExpertReference(editor, {
          refType: "skill",
          id: skill.id,
          label: skill.label,
          uri: null,
          meta: { invocationPrefix: skillPrefix, scope: "expert" },
        })
        syncComposerEmpty()
        handle.focus()
      })
    },
    [skillPrefix, syncComposerEmpty]
  )

  const handleExpertShortcut = useCallback(
    (item: ExpertListItem) => {
      const label =
        pickLocalized(item.metadata.display_name, locale) || item.metadata.id
      if (isSkillLocked(item.metadata.id)) {
        notifySkillNotEnabled(label, item.metadata.id)
        return
      }
      // Experts are open-ended: just the leading badge, no canned template.
      insertSkillShortcut({ id: item.metadata.id, label }, "")
    },
    [locale, isSkillLocked, notifySkillNotEnabled, insertSkillShortcut]
  )

  const handleOfficeShortcut = useCallback(
    (action: OfficeAction) => {
      const label = tQa(action.id as Parameters<typeof tQa>[0])
      if (isSkillLocked(action.skillId)) {
        notifySkillNotEnabled(label, action.skillId)
        return
      }
      insertSkillShortcut(
        { id: action.skillId, label },
        tQa(action.promptKey as Parameters<typeof tQa>[0])
      )
    },
    [tQa, isSkillLocked, notifySkillNotEnabled, insertSkillShortcut]
  )

  const handlePickFiles = useCallback(async () => {
    if (disabled) return
    let picked: string[] = []
    try {
      const { open } = await import("@tauri-apps/plugin-dialog")
      const selected = await open({
        multiple: true,
        directory: false,
        defaultPath:
          getActiveRemoteConnectionId() === null ? defaultPath : undefined,
      })
      if (!selected) return
      picked = (Array.isArray(selected) ? selected : [selected]).filter(
        (item): item is string => !!item
      )
      await appendNativePickedPaths(picked)
    } catch (error) {
      console.error("[MessageInput] pick files failed:", error)
      toast.error(
        tAttach("attachUploadFailed", {
          names: picked.map(fileNameFromPath).join(", ") || "attachment",
        })
      )
    }
  }, [appendNativePickedPaths, defaultPath, disabled, tAttach])

  const handlePickFolder = useCallback(async () => {
    if (disabled) return
    try {
      const localPicker = isDesktop() && getActiveRemoteConnectionId() === null
      const selected = await openFileDialog({
        directory: true,
        multiple: false,
        title: localPicker ? t("referenceFolder") : t("folderReferencePrompt"),
        defaultPath: localPicker ? defaultPath : undefined,
      })
      if (!selected || Array.isArray(selected)) return
      const path = selected.trim()
      if (!isAbsoluteFilesystemPath(path)) {
        toast.error(t("folderReferenceInvalid"))
        return
      }
      insertFileReferences(
        [
          {
            name: directoryNameFromPath(path),
            uri: buildDirectoryUri(path),
            fileKind: "dir",
          },
        ],
        { atCaret: true }
      )
    } catch (error) {
      console.error("[MessageInput] reference folder failed:", error)
      toast.error(t("folderReferenceFailed"))
    }
  }, [defaultPath, disabled, insertFileReferences, t])

  const [serverFilePickerOpen, setServerFilePickerOpen] = useState(false)
  const [projectReferenceOpen, setProjectReferenceOpen] = useState(false)

  const handleProjectReferenceSelect = useCallback(
    (selection: ProjectReferenceSelection) => {
      insertFileReferences(
        [
          {
            name: selection.name,
            uri:
              selection.kind === "dir"
                ? buildDirectoryUri(selection.path)
                : buildFileUri(selection.path),
            fileKind: selection.kind,
          },
        ],
        { atCaret: true }
      )
    },
    [insertFileReferences]
  )

  const handleUploadLocalFiles = useCallback(async () => {
    if (disabled) return
    // Open a hidden <input type="file"> to grab File objects (browsers and
    // Tauri webviews both produce blob-style File objects from this control,
    // never raw OS paths), then upload each one — `uploadAttachment` picks
    // the right transport (direct fetch in web mode, IPC-proxied multipart
    // in remote-desktop mode).
    const input = document.createElement("input")
    input.type = "file"
    input.multiple = true
    input.onchange = async () => {
      const all = input.files ? Array.from(input.files) : []
      await appendFilesFromInput(all)
    }
    input.click()
  }, [appendFilesFromInput, disabled])

  const handleServerFilesSelected = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return
      const ordinary: string[] = []
      for (const path of paths) {
        // The server picker returns paths on the active workspace host. Use
        // the active transport (web or remote desktop), not the local shell.
        if (await appendImagePath(path, "workspace")) continue
        ordinary.push(path)
      }
      if (ordinary.length > 0) appendResourceAttachments(ordinary)
    },
    [appendImagePath, appendResourceAttachments]
  )

  const loadQuickMessages = useCallback(async () => {
    setQuickMessagesLoading(true)
    try {
      const list = await quickMessagesList()
      setQuickMessages(list)
    } catch (error) {
      console.error("[MessageInput] load quick messages failed:", error)
    } finally {
      setQuickMessagesLoading(false)
    }
  }, [])

  const handleAddMenuOpenChange = useCallback(
    (open: boolean) => {
      if (!open) return
      // The editor keeps its selection while the menu is open, so a quick
      // message inserts back at the same caret without tracking an offset.
      loadQuickMessages().catch((error) => {
        console.error("[MessageInput] quick messages refresh failed:", error)
      })
    },
    [loadQuickMessages]
  )

  const handleSkillsMenuOpenChange = useCallback(
    (open: boolean) => {
      if (!open || !agentType) return
      setSkillsMenuScanFailed(false)
      setSkillsMenuScanning(true)
      void refreshAgentSkills(agentType, defaultPath ?? null)
        .catch((error) => {
          setSkillsMenuScanFailed(true)
          console.error("[MessageInput] skill directory scan failed:", error)
        })
        .finally(() => setSkillsMenuScanning(false))
    },
    [agentType, defaultPath]
  )

  const handleQuickMessageSelect = useCallback((message: QuickMessage) => {
    if (!message.content) return
    editorRef.current?.insertTextAtCursor(message.content)
  }, [])

  // Plain-text rendering of the editor's current selection, for the right-click
  // Cut/Copy. Read straight from ProseMirror state (stable while the radix menu
  // holds DOM focus). Use the same leaf mapping as send serialization so copied
  // text matches the wire prompt.
  const selectionPlainText = useCallback((editor: Editor): string => {
    const { from, to } = editor.state.selection
    if (from >= to) return ""
    return editor.state.doc.textBetween(from, to, "\n", composerLeafText)
  }, [])

  // The radix menu traps focus until it closes, so the clipboard write is
  // deferred (see copyTextFromMenu) — otherwise the non-secure execCommand
  // fallback can't focus its scratch textarea. Copy never mutates the document,
  // so a failed write loses nothing; we still surface it (the native menu was
  // suppressed) so the user can fall back to the keyboard.
  const handleContextCopy = useCallback(async () => {
    const editor = editorRef.current?.getEditor()
    if (!editor) return
    const text = selectionPlainText(editor)
    if (!text) return
    if (!(await copyTextFromMenu(text))) {
      toast.error(t("clipboardWriteFailed"))
    }
  }, [selectionPlainText, t])

  const handleContextCut = useCallback(async () => {
    if (disabled) return
    const editor = editorRef.current?.getEditor()
    if (!editor) return
    // Capture the range up front so the post-write delete targets exactly what
    // was copied. Cut is atomic: the deferred clipboard write can fail in a
    // non-secure context, so the range is removed only once the write succeeds —
    // otherwise the selection is kept and the failure is surfaced (no data loss).
    const { from, to } = editor.state.selection
    await cutSelectionToClipboard({
      text: selectionPlainText(editor),
      copy: copyTextFromMenu,
      remove: () => editor.chain().focus().deleteRange({ from, to }).run(),
      onWriteFailed: () => toast.error(t("clipboardWriteFailed")),
    })
  }, [disabled, selectionPlainText, t])

  const handleContextSelectAll = useCallback(() => {
    if (disabled) return
    const editor = editorRef.current?.getEditor()
    if (!editor) return
    editor.chain().focus().selectAll().run()
  }, [disabled])

  // Opening the custom right-click menu: snapshot whether there's a selection
  // (gates Cut/Copy) and refresh the quick-messages list. The editor keeps its
  // selection while the menu is open, so Paste / a quick message lands back at
  // the same caret.
  const handleContextMenuOpenChange = useCallback(
    (open: boolean) => {
      if (!open) return
      const editor = editorRef.current?.getEditor()
      setContextSelectionActive(editor ? !editor.state.selection.empty : false)
      loadQuickMessages().catch((error) => {
        console.error("[MessageInput] quick messages refresh failed:", error)
      })
    },
    [loadQuickMessages]
  )

  // Plain-text ("paste without formatting") paste, shared by the custom
  // right-click menu item and the Ctrl/⌘+Shift+V shortcut. Reads only the
  // clipboard's `text/plain` and inserts it verbatim via `pasteText` (no
  // Markdown/HTML re-parsing), so it strips any formatting a keyboard Ctrl+V
  // would preserve. The native context menu only appears over the
  // contenteditable text, so the blank chrome had no paste affordance — this
  // reproduces the shortcut everywhere in the box. Reading the clipboard happens
  // inside the menu-click / keydown user gesture, so the async Clipboard API has
  // the activation it needs.
  const handleContextPaste = useCallback(async () => {
    if (disabled) return
    const editor = editorRef.current?.getEditor()
    if (!editor) return
    let text = ""
    // The async clipboard read can be blocked at call time even though the API
    // exists (denied permission, browser policy), so track that: with the native
    // menu (and its Paste) suppressed to show this one, a silent failure would
    // leave the user with no feedback and no fallback.
    let readBlocked = false
    try {
      text = (await navigator.clipboard.readText()) ?? ""
    } catch {
      // Permission denied / unsupported / no activation — fall through to the
      // image path (a textless clipboard may still hold a screenshot).
      readBlocked = true
      text = ""
    }
    if (text) {
      // Route through ProseMirror's own text paste so newlines, marks and the
      // editor's paste pipeline behave exactly like a keyboard paste.
      editor.view.focus()
      editor.view.pasteText(text)
      return
    }
    // No text — try a pasted image (screenshot), mirroring `handlePasteFiles`.
    try {
      const imageFiles = await imageFilesFromClipboardApi()
      if (imageFiles.length > 0) {
        await appendFilesFromInput(imageFiles)
        return
      }
    } catch (error) {
      console.error("[MessageInput] context menu paste failed:", error)
      readBlocked = true
    }
    // Nothing landed. A blocked read leaves no visible result and no native menu
    // to retry from, so point the user at the keyboard shortcut. A merely empty
    // clipboard (read succeeded, returned "") stays a silent no-op as before.
    if (readBlocked) {
      toast.error(t("pasteUnavailable"))
    }
  }, [disabled, appendFilesFromInput, t])

  // Bridges the composer's Ctrl/⌘+Shift+V key to the plain-text paste above.
  // Returns whether the shortcut was consumed: when disabled or when the async
  // clipboard read is available we take over (return true) so the composer
  // suppresses the browser's native rich paste; in a non-secure context (no
  // `readText`) we return false so the browser's own "paste and match style"
  // still works. The read runs inside this keydown gesture, so its activation
  // is preserved.
  const handlePlainPasteShortcut = useCallback((): boolean => {
    if (disabled) return true
    if (!clipboardReadSupported) return false
    void handleContextPaste()
    return true
  }, [disabled, clipboardReadSupported, handleContextPaste])

  useEffect(() => {
    if (!attachmentTabId) return

    const handleAttachFile = (event: Event) => {
      const customEvent = event as CustomEvent<AttachFileToSessionDetail>
      if (!customEvent.detail) return
      if (customEvent.detail.tabId !== attachmentTabId) return
      const { path, range } = customEvent.detail
      // Drop the badge at the composer's current caret rather than the end, so
      // "add to chat" / "add file to chat" land where the user left off.
      if (range) {
        appendFileRangeAttachment(path, range, { atCaret: true })
      } else {
        const source =
          getActiveRemoteConnectionId() === null ? "local" : "workspace"
        void appendImagePath(path, source).then((isImage) => {
          if (!isImage) appendResourceAttachments([path], { atCaret: true })
        })
      }
    }

    window.addEventListener(ATTACH_FILE_TO_SESSION_EVENT, handleAttachFile)
    return () => {
      window.removeEventListener(ATTACH_FILE_TO_SESSION_EVENT, handleAttachFile)
    }
  }, [
    appendResourceAttachments,
    appendFileRangeAttachment,
    appendImagePath,
    attachmentTabId,
  ])

  useEffect(() => {
    if (!attachmentTabId) return
    const handleAttachImage = (event: Event) => {
      const detail = (event as CustomEvent<AttachImageToSessionDetail>).detail
      if (!detail || detail.tabId !== attachmentTabId) return
      if (
        !detail.data ||
        !SUPPORTED_IMAGE_MIME_TYPES.has(detail.mimeType.toLowerCase())
      ) {
        console.warn("[MessageInput] ignored invalid image attachment event")
        return
      }
      const file = base64ImageFile({
        data: detail.data,
        mime_type: detail.mimeType.toLowerCase(),
        name: detail.name || "image.png",
      })
      void appendImageFile(file)
      editorRef.current?.focus()
    }
    window.addEventListener(ATTACH_IMAGE_TO_SESSION_EVENT, handleAttachImage)
    return () => {
      window.removeEventListener(
        ATTACH_IMAGE_TO_SESSION_EVENT,
        handleAttachImage
      )
    }
  }, [appendImageFile, attachmentTabId])

  useEffect(() => {
    if (!attachmentTabId) return

    const handleAppendText = (event: Event) => {
      const customEvent = event as CustomEvent<AppendTextToSessionDetail>
      if (!customEvent.detail) return
      if (customEvent.detail.tabId !== attachmentTabId) return
      const appendText = customEvent.detail.text
      const editor = editorRef.current?.getEditor()
      if (!editor) return
      // Append at the very end, separated by a space when the document isn't
      // empty (and doesn't already end in whitespace).
      const ed = editorRef.current
      const needsSpace = ed != null && !ed.isEmpty()
      editor
        .chain()
        .focus("end")
        .insertContent(`${needsSpace ? " " : ""}${appendText}`)
        .run()
    }

    window.addEventListener(APPEND_TEXT_TO_SESSION_EVENT, handleAppendText)
    return () => {
      window.removeEventListener(APPEND_TEXT_TO_SESSION_EVENT, handleAppendText)
    }
  }, [attachmentTabId])

  useEffect(() => {
    let cancelled = false
    const unlisteners: Array<() => void | Promise<void>> = []

    const cleanupListeners = () => {
      for (const fn of unlisteners.splice(0)) {
        disposeTauriListener(fn, "MessageInput.dragDrop")
      }
    }

    type DragDropPayload =
      | {
          type: "enter" | "drop"
          paths: string[]
          position: { x: number; y: number }
        }
      | {
          type: "over"
          position: { x: number; y: number }
        }
      | { type: "leave" }

    const handlePayload = (payload: DragDropPayload) => {
      const host = containerRef.current
      if (!host) return
      if (payload.type === "leave") {
        setDragActiveIfChanged(false)
        return
      }
      const inside = pointWithinElement(payload.position, host)
      if (payload.type === "drop") {
        setDragActiveIfChanged(false)
        if (Date.now() - lastDomDropAtRef.current < 250) return
        if (!inside || disabledRef.current) return
        if (getActiveRemoteConnectionId() !== null) {
          // Remote workspace: local OS paths are unreachable from the
          // remote agent, so stream the bytes through the upload proxy and
          // attach the resulting server-side paths instead.
          void (async () => {
            const ordinary = await collectOrdinaryNativePaths(
              payload.paths,
              true
            )
            if (ordinary.length > 0) {
              await uploadPathsToRemoteRef.current(ordinary)
            }
          })().catch((error) => {
            console.error(
              "[MessageInput] remote drag-drop upload failed:",
              error
            )
          })
          return
        }
        void (async () => {
          const ordinary = await collectOrdinaryNativePaths(
            payload.paths,
            false
          )
          if (ordinary.length > 0) {
            await appendPathsFromDropRef.current(ordinary)
          }
        })().catch((error) => {
          console.error("[MessageInput] drag drop paths failed:", error)
        })
        return
      }
      setDragActiveIfChanged(inside && !disabledRef.current)
    }

    const setup = async () => {
      if (!isDesktop()) return
      const { getCurrentWebview } = await import("@tauri-apps/api/webview")
      const { TauriEvent } = await import("@tauri-apps/api/event")
      const webview = getCurrentWebview()
      try {
        const unlistenEnter = await webview.listen<{
          paths: string[]
          position: { x: number; y: number }
        }>(TauriEvent.DRAG_ENTER, (event) => {
          if (cancelled) return
          handlePayload({
            type: "enter",
            paths: event.payload.paths,
            position: event.payload.position,
          })
        })
        unlisteners.push(unlistenEnter)

        const unlistenOver = await webview.listen<{
          position: { x: number; y: number }
        }>(TauriEvent.DRAG_OVER, (event) => {
          if (cancelled) return
          handlePayload({
            type: "over",
            position: event.payload.position,
          })
        })
        unlisteners.push(unlistenOver)

        const unlistenDrop = await webview.listen<{
          paths: string[]
          position: { x: number; y: number }
        }>(TauriEvent.DRAG_DROP, (event) => {
          if (cancelled) return
          handlePayload({
            type: "drop",
            paths: event.payload.paths,
            position: event.payload.position,
          })
        })
        unlisteners.push(unlistenDrop)

        const unlistenLeave = await webview.listen(
          TauriEvent.DRAG_LEAVE,
          () => {
            if (cancelled) return
            handlePayload({ type: "leave" })
          }
        )
        unlisteners.push(unlistenLeave)
      } catch {
        // Ignore non-Tauri environments.
      } finally {
        if (cancelled) {
          cleanupListeners()
        }
      }
    }

    void setup()

    return () => {
      cancelled = true
      cleanupListeners()
    }
  }, [collectOrdinaryNativePaths, setDragActiveIfChanged])

  const removeAttachment = useCallback(
    (id: string) => {
      setAttachments((prev) => prev.filter((item) => item.id !== id))
    },
    [setAttachments]
  )

  const retryImageStaging = useCallback(
    async (id: string) => {
      const attachment = attachments.find(
        (item): item is ImageInputAttachment =>
          item.type === "image" && item.id === id
      )
      if (!attachment?.staging || attachment.staging.status === "uploading") {
        return
      }
      const source = attachment.staging.source
      setAttachments((current) =>
        updateImageAttachment(current, id, (item) => ({
          ...item,
          staging: { status: "uploading", source },
        }))
      )
      try {
        const staged = await retryImageUpload(source, {
          ...chatImageStorage,
          mimeType: attachment.sourceMimeType,
        })
        setAttachments((current) =>
          applyPreparedImage(current, id, staged, attachment.sourceMimeType)
        )
      } catch (error) {
        console.error("[MessageInput] image staging retry failed", {
          name: attachment.name,
          error,
        })
        setAttachments((current) =>
          updateImageAttachment(current, id, (item) => ({
            ...item,
            staging: { status: "failed", source },
          }))
        )
        toast.error(tAttach("attachUploadFailed", { names: attachment.name }))
      }
    },
    [attachments, chatImageStorage, setAttachments, tAttach]
  )

  const applyAttachmentEdit = useCallback(
    async (result: EditorImageResult) => {
      if (
        !previewAttachmentId ||
        !attachmentsRef.current.some(
          (item) => item.type === "image" && item.id === previewAttachmentId
        )
      ) {
        return
      }
      const file = base64ImageFile(result)
      const previewUrl = URL.createObjectURL(file)
      const source: ImageAttachmentStaging["source"] = {
        kind: "browser-file",
        file,
      }
      setAttachments((current) =>
        updateImageAttachment(current, previewAttachmentId, (item) => ({
          ...item,
          data: "",
          uri: null,
          localPath: null,
          name: result.name,
          mimeType: result.mime_type,
          sourceMimeType: result.mime_type,
          previewUrl,
          staging: { status: "uploading", source },
        }))
      )
      try {
        const prepared = await uploadChatImage(file, {
          ...chatImageStorage,
          mimeType: result.mime_type,
        })
        setAttachments((current) =>
          applyPreparedImage(
            current,
            previewAttachmentId,
            prepared,
            result.mime_type
          )
        )
      } catch (error) {
        console.error("[MessageInput] edited image staging failed", {
          name: result.name,
          error,
        })
        setAttachments((current) =>
          updateImageAttachment(current, previewAttachmentId, (item) => ({
            ...item,
            staging: { status: "failed", source },
          }))
        )
        throw error
      }
    },
    [chatImageStorage, previewAttachmentId, setAttachments]
  )

  const buildDraft = useCallback((): PromptDraft | null => {
    const editor = editorRef.current?.getEditor()
    // The send boundary is authoritative in case the agent changed before the
    // deferred re-stamp effect ran.
    if (editor) {
      normalizeDirectiveReferences(editor)
      restampSkillPrefixes(editor, skillPrefix)
    }
    // Inline badges + prose → text/resource_link blocks (file mentions become
    // first-class ResourceLinks; agent/session/commit/skill stay inline text;
    // embedded badges are dropped here and re-added below from the payload map).
    const blocks: PromptInputBlock[] = editor ? docToPromptBlocks(editor) : []
    const missingVariables =
      editorRef.current?.getUnfilledScenarioVariables() ?? []
    if (missingVariables.length > 0) {
      toast.error(
        t("scenarioVariablesRequired", {
          variables: missingVariables.join("、"),
        })
      )
      return null
    }
    // Keep embedded attachment badges visible in the optimistic bubble even
    // though their synthetic URI is omitted from the wire text.
    const displayProse = editor
      ? serializeDocToDisplayText(editor.state.doc).trim()
      : ""
    // Append the real bytes-bearing block for every embedded-attachment badge
    // still present in the document, looked up by its `iyw-claw://embedded/…` uri.
    // Walking the live doc (rather than a swap pass over a stored draft) means a
    // deleted badge's stale map entry is simply never emitted, and an undo that
    // resurrects a badge re-emits it — no pruning, and no orphan uri can leak.
    if (editor) {
      editor.state.doc.descendants((node) => {
        if (
          node.type.name === "reference" &&
          typeof node.attrs?.uri === "string" &&
          isEmbeddedReferenceUri(node.attrs.uri)
        ) {
          const real = embeddedPayloadsRef.current.get(node.attrs.uri)
          if (real) blocks.push(real)
        }
        return true
      })
    }
    const invalidImage = attachments.find(
      (attachment): attachment is ImageInputAttachment =>
        attachment.type === "image" &&
        (!SUPPORTED_IMAGE_MIME_TYPES.has(attachment.mimeType.toLowerCase()) ||
          !isPublicImageUrl(attachment.uri) ||
          attachment.data.length > 0)
    )
    if (invalidImage) {
      console.error("[MessageInput] send blocked invalid image attachment", {
        name: invalidImage.name,
        mimeType: invalidImage.mimeType,
        base64Length: invalidImage.data.length,
      })
      toast.error(tAttach("attachImageReadFailed", { name: invalidImage.name }))
      return null
    }
    const unstagedImage = attachments.find(
      (attachment): attachment is ImageInputAttachment =>
        attachment.type === "image" && attachment.staging !== undefined
    )
    if (unstagedImage) {
      toast.error(
        tAttach("attachImageStagingRequired", { name: unstagedImage.name })
      )
      return null
    }
    if (blocks.length === 0 && attachments.length === 0) return null

    // `attachments` holds only images now — files live inline as badges above.
    for (const attachment of attachments) {
      if (attachment.type === "image") {
        blocks.push({
          type: "image",
          data: "",
          mime_type: attachment.mimeType,
          uri: attachment.uri,
          local_path: attachment.localPath ?? null,
        })
      }
    }

    const displayText =
      displayProse ||
      `Attached ${attachments.length} attachment${attachments.length > 1 ? "s" : ""}`
    console.info("[MessageInput] draft image boundary", {
      imageCount: attachments.filter(
        (attachment) => attachment.type === "image"
      ).length,
      resourceCount: blocks.filter((block) => block.type !== "image").length,
    })
    const expert = editor ? getExpertReference(editor) : null
    const packageMeta = expert?.meta
    const skillPackage =
      packageMeta?.marketSkillId &&
      packageMeta.marketSkillSlug &&
      packageMeta.marketSkillVersion
        ? {
            id: packageMeta.marketSkillId,
            slug: packageMeta.marketSkillSlug,
            version: packageMeta.marketSkillVersion,
          }
        : undefined
    return { blocks, displayText, skillPackage }
  }, [attachments, skillPrefix, t, tAttach])

  // Clear the accepted draft immediately. Invalidate the pre-send debounce so
  // it cannot write the old document back after the visible composer is empty.
  const resetComposer = useCallback(() => {
    cancelPendingDraftSave()
    if (effectiveDraftStorageKey && !isEditingQueueItem) {
      clearMessageInputDraftV2(effectiveDraftStorageKey)
    }
    programmaticResetRef.current = true
    try {
      editorRef.current?.clear()
      setAttachments([])
    } finally {
      programmaticResetRef.current = false
    }
    setComposerEmpty(true)
    setDraftTask(null)
    embeddedPayloadsRef.current.clear()
    closeSlashMenu()
  }, [
    cancelPendingDraftSave,
    closeSlashMenu,
    effectiveDraftStorageKey,
    isEditingQueueItem,
    setAttachments,
  ])

  const capturePendingSend = useCallback(
    (draft: PromptDraft): PendingSendSnapshot => {
      return {
        doc: editorRef.current?.getJSON() ?? { type: "doc", content: [] },
        attachments: attachmentsRef.current.slice(),
        embeddedPayloads: new Map(embeddedPayloadsRef.current),
        composerInstanceId: composerInstanceIdRef.current,
        fallbackText: draft.displayText.trim(),
        mutationVersion: composerMutationVersionRef.current,
      }
    },
    []
  )

  const restoreRejectedSend = useCallback(
    (snapshot: PendingSendSnapshot) => {
      const editor = editorRef.current
      const restoredEditor = editor?.getEditor()
      if (!editor || !restoredEditor) return false
      const sameInstance =
        snapshot.composerInstanceId === composerInstanceIdRef.current
      const unchanged = sameInstance
        ? composerMutationVersionRef.current === snapshot.mutationVersion
        : composerMutationVersionRef.current === 0
      const onlyFailureFallback =
        !sameInstance &&
        unchanged &&
        attachmentsRef.current.length === 0 &&
        editor.getText().trim() === snapshot.fallbackText
      if (
        !unchanged ||
        (!editor.isEmpty() && !onlyFailureFallback) ||
        attachmentsRef.current.length > 0
      ) {
        return true
      }
      embeddedPayloadsRef.current = new Map(snapshot.embeddedPayloads)
      editor.setDoc(snapshot.doc)
      setAttachments(snapshot.attachments)
      setComposerEmpty(editor.isEmpty())
      setDraftTask(getTaskReference(restoredEditor))
      editor.focus()
      return true
    },
    [setAttachments]
  )

  const pendingSendScope = attachmentTabId ?? effectiveDraftStorageKey
  useEffect(() => {
    if (!pendingSendScope || !composerHydrated) return
    return subscribePendingSendRestore(pendingSendScope, restoreRejectedSend)
  }, [composerHydrated, pendingSendScope, restoreRejectedSend])

  const rejectPendingSend = useCallback(
    (snapshot: PendingSendSnapshot) => {
      if (pendingSendScope) {
        requestAnimationFrame(() => {
          publishPendingSendRestore(pendingSendScope, snapshot)
        })
      } else {
        restoreRejectedSend(snapshot)
      }
    },
    [pendingSendScope, restoreRejectedSend]
  )

  const sendCurrentDraft = useCallback(() => {
    // The editor stays editable while `disabled` (the agent is busy) so the user
    // can keep typing, but a plain send is blocked — only enqueue / queue-edit
    // save go through. Mirrors the legacy textarea's keydown guard.
    if (disabled && !isPrompting && !isEditingQueueItem) {
      return
    }
    const draft = buildDraft()
    if (!draft) return
    const editor = editorRef.current?.getEditor()
    const sentTask = editor ? getTaskReference(editor) : null

    // Edit mode: save back to queue item
    if (isEditingQueueItem && onSaveQueueEdit) {
      onSaveQueueEdit(draft)
      resetComposer()
      return
    }

    // Prompting mode: enqueue instead of sending
    if (isPrompting) {
      if (!onEnqueue || sendPendingRef.current) return
      const snapshot = capturePendingSend(draft)
      const result = onEnqueue(draft, showModeSelector ? effectiveModeId : null)
      if (result === false) return
      resetComposer()
      if (result instanceof Promise) {
        sendPendingRef.current = true
        setSendPending(true)
        void result
          .then((accepted) => {
            if (accepted === false) rejectPendingSend(snapshot)
          })
          .catch((error) => {
            console.error("[MessageInput] enqueue preflight failed", { error })
            rejectPendingSend(snapshot)
          })
          .finally(() => {
            sendPendingRef.current = false
            setSendPending(false)
          })
      }
      return
    }

    // Only a direct send is single-flighted. Queue edits and prompting-time
    // enqueue actions above remain usable while the previous send is accepted.
    if (sendPendingRef.current) return

    if (
      !promptCapabilities.image &&
      draft.blocks.some((block) => block.type === "image")
    ) {
      console.info("[MessageInput] host image pre-analysis required", {
        imageCount: draft.blocks.filter((block) => block.type === "image")
          .length,
      })
    }
    const snapshot = capturePendingSend(draft)
    const sendGeneration = ++sendGenerationRef.current
    const trackSentTask = !hasQueuedMessages
    setRunningTask(null)
    sendPendingRef.current = true
    let result: boolean | void | Promise<boolean>
    try {
      result = onSend(draft, showModeSelector ? effectiveModeId : null)
    } catch (error) {
      console.error("[MessageInput] send failed before acceptance", { error })
      sendPendingRef.current = false
      resetComposer()
      rejectPendingSend(snapshot)
      return
    }
    if (result === false) {
      sendPendingRef.current = false
      return
    }
    resetComposer()
    if (result === undefined || result === true) {
      if (trackSentTask) setRunningTask(sentTask)
      sendPendingRef.current = false
      return
    }
    setSendPending(true)
    void result
      .then((accepted) => {
        if (accepted === false) rejectPendingSend(snapshot)
        else if (
          trackSentTask &&
          sendGenerationRef.current === sendGeneration
        ) {
          setRunningTask(sentTask)
        }
      })
      .catch((error) => {
        console.error("[MessageInput] send failed before acceptance", { error })
        rejectPendingSend(snapshot)
      })
      .finally(() => {
        sendPendingRef.current = false
        setSendPending(false)
      })
  }, [
    disabled,
    buildDraft,
    isEditingQueueItem,
    isPrompting,
    onSaveQueueEdit,
    onEnqueue,
    onSend,
    effectiveModeId,
    showModeSelector,
    resetComposer,
    capturePendingSend,
    rejectPendingSend,
    promptCapabilities.image,
    hasQueuedMessages,
  ])

  const handleVoiceFinal = useCallback((text: string) => {
    editorRef.current?.appendText(text)
  }, [])

  const handleVoiceError = useCallback(
    (kind: RealtimeVoiceErrorKind) => {
      toast.error(t(`voice.${kind}`))
    },
    [t]
  )

  const voice = useRealtimeVoiceInput({
    enabled: desktopMode && !isEditingQueueItem,
    scopeKey: attachmentTabId ?? effectiveDraftStorageKey,
    onFinal: handleVoiceFinal,
    onAutoSend: sendCurrentDraft,
    onError: handleVoiceError,
  })

  const handleSend = useCallback(() => {
    if (voice.status !== "idle") return
    sendCurrentDraft()
  }, [sendCurrentDraft, voice.status])

  const handleForkSendClick = useCallback(() => {
    if (voice.status !== "idle") return
    if (!onForkSend) return
    const draft = buildDraft()
    if (!draft) return
    // Fork-send consumes the draft synchronously, exactly like a normal send:
    // fire-and-forget and clear the input immediately, so there is no in-flight
    // editable window. If the fork can't run (queue non-empty / disconnected /
    // failure) the parent re-queues the draft, so it is never lost.
    const accepted = onForkSend(
      draft,
      showModeSelector ? effectiveModeId : null
    )
    if (accepted === false) return
    resetComposer()
  }, [
    onForkSend,
    buildDraft,
    effectiveModeId,
    showModeSelector,
    resetComposer,
    voice.status,
  ])

  // Navigation/confirm/escape keys for the `/` (commands) and `$` (Codex skills)
  // runtime menu, routed from inside the editor (RichComposer.onExternalMenuKeyDown)
  // because ProseMirror's DOM handler fires before a host capture handler could.
  // Returns true for keys the menu consumed; false (e.g. a letter that filters)
  // lets normal editing proceed.
  const handleExternalMenuKeyDown = useCallback(
    (event: KeyboardEvent): boolean => {
      if (event.isComposing) return false
      if (!slashMenuOpen || slashAutocompleteCount === 0) return false
      if (event.key === "ArrowDown") {
        setSlashSelectedIndex((i) =>
          i < slashAutocompleteCount - 1 ? i + 1 : 0
        )
        return true
      }
      if (event.key === "ArrowUp") {
        setSlashSelectedIndex((i) =>
          i > 0 ? i - 1 : slashAutocompleteCount - 1
        )
        return true
      }
      if (event.key === "Enter" || event.key === "Tab") {
        // The merged list is [commands, skills].
        if (slashSelectedIndex < filteredSlashCommands.length) {
          handleSlashSelect(filteredSlashCommands[slashSelectedIndex])
        } else {
          const skill =
            filteredSlashSkills[
              slashSelectedIndex - filteredSlashCommands.length
            ]
          if (skill) handleSkillAutocompleteSelect(skill)
        }
        return true
      }
      if (event.key === "Escape") {
        closeSlashMenu()
        return true
      }
      return false
    },
    [
      slashMenuOpen,
      slashAutocompleteCount,
      slashSelectedIndex,
      filteredSlashCommands,
      filteredSlashSkills,
      handleSlashSelect,
      handleSkillAutocompleteSelect,
      closeSlashMenu,
    ]
  )

  // Escape cancels a queue edit. ProseMirror doesn't consume Escape, so it
  // bubbles up to this container handler. Skipped while the slash menu is open
  // (the editor handles that Escape to close the menu first).
  const handleContainerKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.nativeEvent.isComposing) return
      if (
        isEditingQueueItem &&
        e.key === "Escape" &&
        !slashMenuOpen &&
        onCancelQueueEdit
      ) {
        e.preventDefault()
        onCancelQueueEdit()
      }
    },
    [isEditingQueueItem, slashMenuOpen, onCancelQueueEdit]
  )

  // Clicking the input's empty chrome (its padding, the blank space below a
  // short message, the gaps in the action bar) focuses the editor — previously
  // only the editor surface itself was clickable. Interactive controls, inline
  // badges and the editor surface handle their own clicks, so they're excluded;
  // `preventDefault` keeps the editor from blurring before we refocus it. We
  // focus *at the click point* (not the end of the document) so clicking the
  // left/top padding next to existing text lands the caret there, like a native
  // textarea, instead of always jumping to the end.
  const handleChromeMouseDown = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      // Not gated on `disabled`: the editor stays editable while connecting (see
      // `handleSend`), so chrome clicks must focus too — else only the existing
      // text line is clickable and the blank area below it is dead until ready.
      if (!isComposerChromeClick(e.target)) return
      // Keep the editor from blurring before we refocus it.
      e.preventDefault()
      editorRef.current?.focusAtCoords(e.clientX, e.clientY)
    },
    []
  )

  const handleContainerDragOver = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (!hasDragFiles(event.dataTransfer)) return
      event.preventDefault()
      if (!disabled) {
        setDragActiveIfChanged(true)
      }
    },
    [disabled, setDragActiveIfChanged]
  )

  const handleContainerDragLeave = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      const related = event.relatedTarget
      if (
        related &&
        related instanceof Node &&
        event.currentTarget.contains(related)
      ) {
        return
      }
      setDragActiveIfChanged(false)
    },
    [setDragActiveIfChanged]
  )

  const handleContainerDrop = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (!hasDragFiles(event.dataTransfer)) return
      event.preventDefault()
      setDragActiveIfChanged(false)
      if (disabled) return
      const files = Array.from(event.dataTransfer.files ?? [])
      // Tauri's native drop event carries real OS paths. When the DOM File
      // objects omit those paths, let the native event own the drop so large
      // local files can remain direct references instead of being rejected by
      // the byte-upload path.
      if (
        showNativePaperclip &&
        (files.length === 0 || files.some((file) => getFilePath(file) === null))
      ) {
        lastDomDropAtRef.current = 0
        return
      }
      lastDomDropAtRef.current = Date.now()
      if (files.length > 0) {
        void appendFilesFromInput(files).catch((error) => {
          console.error("[MessageInput] drop files failed:", error)
        })
      }
    },
    [
      appendFilesFromInput,
      disabled,
      setDragActiveIfChanged,
      showNativePaperclip,
    ]
  )

  const hasImageAttachments = imageAttachments.length > 0
  const showDragActive = isDragActive && !disabled
  const visibleTask = draftTask ?? (isPrompting ? runningTask : null)

  const inlineSelectorItems = (
    <>
      {showModeSelector && (
        <InlineModeSelector
          modes={availableModes}
          selectedModeId={effectiveModeId!}
          onSelect={handleModeSelect}
          label={t("modeLabel")}
        />
      )}
      {hasConfigOptions &&
        orderedConfigOptions.map((option) => {
          if (isModelConfigOption(option)) {
            return (
              <ModelOptionPicker
                key={option.id}
                option={option}
                groups={modelListGroups(option)}
                behaviorOptions={modelBehaviorOptions}
                onSelect={(configId, valueId) =>
                  onConfigOptionChange?.(configId, valueId)
                }
                onBehaviorSelect={(configId, valueId) =>
                  onConfigOptionChange?.(configId, valueId)
                }
              />
            )
          }
          return (
            <InlineSessionConfigSelector
              key={option.id}
              option={option}
              derivedGroups={deriveModelGroups(option)}
              onSelect={(configId, valueId) =>
                onConfigOptionChange?.(configId, valueId)
              }
            />
          )
        })}
    </>
  )

  // Normalized settings for the collapsed (narrow) master–detail panel.
  // Mode and config selectors share the same product-defined order.
  const collapsedSettings = useMemo<SessionSelectorSetting[]>(() => {
    const result: SessionSelectorSetting[] = []
    if (showModeSelector) {
      const selected = availableModes.find(
        (mode) => mode.id === effectiveModeId
      )
      result.push({
        key: "mode",
        title: t("modeLabel"),
        currentValue: effectiveModeId ?? "",
        currentLabel: selected?.name ?? effectiveModeId ?? "",
        groups: [
          {
            key: "__modes__",
            name: null,
            options: availableModes.map((mode) => ({
              value: mode.id,
              name: mode.name,
              description: mode.description,
            })),
          },
        ],
        onSelect: (value) => handleModeSelect(value),
      })
    }
    if (hasConfigOptions) {
      for (const option of orderedConfigOptions) {
        if (option.kind.type !== "select") continue
        const kind = option.kind
        // Model values that carry a `provider/` prefix group by provider; every
        // other option keeps its server groups or stays flat (`null` derived).
        const derived = deriveModelGroups(option)
        const groups: SessionSelectorGroup[] = derived
          ? derived.map((group) => ({
              key: group.key,
              name: group.name,
              options: group.options.map((item) => ({
                value: item.value,
                name: item.name,
                description: item.description,
                iconUrl: item.iconUrl,
                modelBehavior: item.modelBehavior,
              })),
            }))
          : kind.groups.length > 0
            ? kind.groups.map((group) => ({
                key: group.group,
                name: group.name,
                options: group.options.map((item) => ({
                  value: item.value,
                  name: item.name,
                  description: item.description,
                  iconUrl: item.iconUrl,
                  modelBehavior: item.modelBehavior,
                })),
              }))
            : [
                {
                  key: "__flat__",
                  name: null,
                  options: kind.options.map((item) => ({
                    value: item.value,
                    name: item.name,
                    description: item.description,
                    iconUrl: item.iconUrl,
                    modelBehavior: item.modelBehavior,
                  })),
                },
              ]
        // Resolve the left-rail summary against the built groups so a grouped
        // model shows its prefix-stripped name (the provider is implied) rather
        // than repeating `provider/`.
        const current = groups
          .flatMap((group) => group.options)
          .find((item) => item.value === kind.current_value)
        // A long model list gets a searchable + virtualized detail pane (a plain
        // list of hundreds of buttons janks); short lists keep plain buttons.
        const searchable = isModelConfigOption(option)
        result.push({
          key: `config:${option.id}`,
          title: option.name,
          currentValue: kind.current_value,
          currentLabel: current?.name ?? kind.current_value,
          groups,
          onSelect: (value) => onConfigOptionChange?.(option.id, value),
          ...(isModelConfigOption(option) && {
            modelBehaviorOptions,
            onModelBehaviorSelect: (configId: string, valueId: string) =>
              onConfigOptionChange?.(configId, valueId),
          }),
          ...(searchable && {
            search: {
              placeholder: t("searchModel"),
              inputLabel: t("searchModelAria"),
              listLabel: t("modelListLabel"),
              empty: t("noModels"),
            },
          }),
        })
      }
    }
    return result
  }, [
    hasConfigOptions,
    modelBehaviorOptions,
    orderedConfigOptions,
    showModeSelector,
    availableModes,
    effectiveModeId,
    onConfigOptionChange,
    handleModeSelect,
    t,
  ])

  const actionButtons = isEditingQueueItem ? (
    <div className="flex items-center gap-1">
      <Button
        onClick={onCancelQueueEdit}
        variant="ghost"
        size="icon"
        className="h-8 w-8"
        title={tQueue("cancelEdit")}
      >
        <X className="size-4" />
      </Button>
      <Button
        onClick={handleSend}
        disabled={!hasSendableContent || hasUnstagedImage}
        size="icon"
        className="h-8 w-8"
        title={tQueue("saveEdit")}
      >
        <Check className="size-4" />
      </Button>
    </div>
  ) : isPrompting ? (
    <Button
      onClick={onCancel}
      disabled={!onCancel}
      size="icon"
      className="iyw-claw-prompting-button iyw-claw-send-button h-8 w-8"
      title={t("stopGeneration")}
      aria-label={t("stopGeneration")}
    >
      <Square className="size-3.5 fill-current" />
    </Button>
  ) : onForkSend ? (
    <div className="flex items-center">
      <Button
        onClick={handleSend}
        disabled={
          sendPending ||
          disabled ||
          voice.status !== "idle" ||
          !hasSendableContent ||
          hasUnstagedImage
        }
        size="icon"
        className="iyw-claw-send-button h-8 w-8 rounded-r-none"
        title={t("send")}
      >
        <Send className="size-4" />
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            disabled={
              sendPending ||
              disabled ||
              voice.status !== "idle" ||
              !hasSendableContent ||
              hasUnstagedImage
            }
            size="icon"
            className="h-8 w-5 rounded-l-none border-l border-primary-foreground/20"
            aria-label={t("forkAndSend")}
          >
            <ChevronUp className="size-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" side="top">
          <DropdownMenuItem onSelect={handleForkSendClick}>
            <GitFork className="h-4 w-4" />
            {t("forkAndSend")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  ) : (
    <Button
      onClick={handleSend}
      disabled={
        sendPending ||
        disabled ||
        voice.status !== "idle" ||
        !hasSendableContent ||
        hasUnstagedImage
      }
      size="icon"
      className="iyw-claw-send-button h-8 w-8"
      title={t("send")}
    >
      <Send className="size-4" />
    </Button>
  )

  return (
    <div
      ref={containerRef}
      className="relative"
      onKeyDown={handleContainerKeyDown}
      onDragOver={handleContainerDragOver}
      onDragLeave={handleContainerDragLeave}
      onDrop={handleContainerDrop}
    >
      {slashMenuOpen && slashAutocompleteCount > 0 && (
        <div className="absolute bottom-full left-0 right-0 mb-1 z-50 flex max-h-[min(16rem,40dvh)] flex-col overflow-hidden rounded-xl border border-border bg-popover shadow-lg">
          {/* No search box: the user types the filter inline after `/` (like the
              `@` panel); navigation is routed from the editor's keydown. */}
          <div ref={slashMenuListRef} className="flex-1 overflow-y-auto p-1">
            {filteredSlashCommands.map((cmd, i) => (
              <button
                key={`cmd-${cmd.name}`}
                type="button"
                className={cn(
                  "flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm",
                  i === slashSelectedIndex
                    ? "bg-accent text-accent-foreground"
                    : "hover:bg-muted"
                )}
                onMouseDown={(e) => {
                  e.preventDefault()
                  handleSlashSelect(cmd)
                }}
              >
                <span className="shrink-0 font-mono text-primary">
                  /{cmd.name}
                </span>
                <span className="truncate text-xs text-muted-foreground">
                  {cmd.description}
                </span>
              </button>
            ))}
            {filteredSlashSkills.map((skill, i) => {
              const absoluteIndex = filteredSlashCommands.length + i
              return (
                <button
                  key={`skill-${skill.scope}-${skill.id}`}
                  type="button"
                  className={cn(
                    "flex w-full items-start gap-2 rounded-lg px-3 py-2 text-left text-sm",
                    absoluteIndex === slashSelectedIndex
                      ? "bg-accent text-accent-foreground"
                      : "hover:bg-muted"
                  )}
                  onMouseDown={(e) => {
                    e.preventDefault()
                    handleSkillAutocompleteSelect(skill)
                  }}
                >
                  <BookOpenText className="mt-0.5 size-4 shrink-0 text-primary/80" />
                  <div className="flex min-w-0 flex-1 items-center gap-2">
                    <span className="shrink-0 font-medium">{skill.name}</span>
                    <span
                      className="min-w-0 flex-1 truncate text-xs text-muted-foreground"
                      title={skill.description ?? undefined}
                    >
                      {skill.description ?? `${skillPrefix}${skill.id}`}
                    </span>
                  </div>
                </button>
              )
            })}
          </div>
        </div>
      )}
      <div
        className={cn(
          folderBranchPickerAttached
            ? "overflow-hidden rounded-xl transition-colors"
            : "contents",
          folderBranchPickerAttached &&
            showDragActive &&
            "ring-1 ring-primary/40"
        )}
      >
        <ContextMenu onOpenChange={handleContextMenuOpenChange}>
          {/* Disabled in non-secure web (no async clipboard read) so the native
              context menu — whose Paste still works over the editor text — is
              not suppressed. Desktop/secure-web get the full custom menu. */}
          <ContextMenuTrigger asChild disabled={!clipboardReadSupported}>
            <div
              onMouseDown={handleChromeMouseDown}
              className={cn(
                // `iyw-claw-composer-chrome` paints the text I-beam across the box's
                // blank areas (padding, the dead space below a short message, the
                // action-bar gaps) so the whole input reads as clickable-to-type;
                // interactive controls re-assert their own cursor (see globals.css).
                "iyw-claw-composer-chrome @container relative flex flex-col rounded-xl border border-input bg-transparent transition-colors",
                // Standard focus ring — always shown when the composer is
                // focused (the plain default input style).
                folderBranchPickerAttached
                  ? "bg-background focus-within:border-ring focus-within:ring-[3px] focus-within:ring-inset focus-within:ring-ring/50"
                  : "focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50",
                // Active session, tiled across multiple sessions: a gradient
                // flows around the border to mark which tile is active — but ONLY
                // while the composer itself is not focused. Focusing it hides the
                // flow (globals.css) so the default focus ring above takes over.
                // A lone/non-tiled session (showActiveFlow=false) and inactive
                // tiles show the plain default border.
                isPrompting
                  ? "iyw-claw-composer-prompting"
                  : showActiveFlow && "iyw-claw-composer-flow",
                !folderBranchPickerAttached &&
                  showDragActive &&
                  "ring-1 ring-primary/40",
                className
              )}
            >
              <ConversationContextBar
                hasExtraContent={hasImageAttachments}
                scrollEndTrigger={attachments.length}
                extraContent={
                  <>
                    {imageAttachments.map((attachment) => {
                      const imageSrc = imageAttachmentSrc(attachment)
                      return (
                        <div
                          key={attachment.id}
                          className={cn(
                            "relative shrink-0 overflow-hidden rounded-md border bg-muted/30",
                            attachment.staging?.status === "failed"
                              ? "border-amber-500/70"
                              : "border-border/70"
                          )}
                        >
                          <button
                            type="button"
                            onClick={() =>
                              setPreviewAttachmentId(attachment.id)
                            }
                            disabled={!imageSrc}
                            className="cursor-pointer transition-opacity hover:opacity-80 disabled:cursor-default"
                          >
                            {imageSrc ? (
                              <Image
                                src={imageSrc}
                                alt={attachment.name}
                                width={56}
                                height={56}
                                unoptimized
                                className="h-14 w-14 object-cover"
                              />
                            ) : (
                              <span className="flex h-14 w-14 items-center justify-center text-muted-foreground">
                                <FileImage className="h-5 w-5" aria-hidden />
                              </span>
                            )}
                          </button>
                          {attachment.staging?.status === "uploading" ? (
                            <span
                              className="pointer-events-none absolute bottom-1 right-1 rounded-sm bg-background/85 p-0.5 shadow-sm"
                              role="status"
                              aria-label={t("attachUploading", {
                                name: attachment.name,
                              })}
                              title={t("attachUploading", {
                                name: attachment.name,
                              })}
                            >
                              <LoaderCircle
                                className="h-3 w-3 animate-spin"
                                aria-hidden
                              />
                            </span>
                          ) : attachment.staging ? (
                            <>
                              <span
                                className="pointer-events-none absolute bottom-1 left-1 rounded-sm bg-background/85 p-0.5 text-amber-600 shadow-sm"
                                role="img"
                                aria-label={t("attachUploadFailed", {
                                  names: attachment.name,
                                })}
                                title={t("attachUploadFailed", {
                                  names: attachment.name,
                                })}
                              >
                                <TriangleAlert
                                  className="h-3 w-3"
                                  aria-hidden
                                />
                                <span className="sr-only">
                                  {t("attachUploadFailed", {
                                    names: attachment.name,
                                  })}
                                </span>
                              </span>
                              <button
                                type="button"
                                onClick={() =>
                                  void retryImageStaging(attachment.id)
                                }
                                className="absolute bottom-1 right-1 rounded-sm bg-background/85 p-0.5 shadow-sm hover:bg-background"
                                aria-label={t("retryAttachment", {
                                  name: attachment.name,
                                })}
                                title={t("retryAttachment", {
                                  name: attachment.name,
                                })}
                              >
                                <RotateCcw className="h-3 w-3" />
                              </button>
                            </>
                          ) : null}
                          <button
                            type="button"
                            onClick={() => removeAttachment(attachment.id)}
                            className="absolute right-1 top-1 rounded-sm bg-background/70 p-0.5 hover:bg-background"
                            aria-label={t("removeAttachmentAria", {
                              name: attachment.name,
                            })}
                          >
                            <X className="h-3 w-3" />
                          </button>
                        </div>
                      )
                    })}
                  </>
                }
              />
              {visibleTask && (
                <TaskModeRail
                  task={visibleTask}
                  running={
                    draftTask == null && runningTask != null && isPrompting
                  }
                  commands={slashCommands}
                  skills={visibleEnabledSkills}
                  skillPrefix={skillPrefix}
                  onSelect={handleTaskCommandSelect}
                  onRemove={handleTaskRemove}
                />
              )}
              <RichComposer
                ref={editorRef}
                placeholder={resolvedPlaceholder}
                ariaLabel={resolvedPlaceholder}
                autoFocus={autoFocus}
                referenceSearch={referenceSearch}
                onReferenceSelect={handleComposerReferenceSelect}
                mentionUiLabels={mentionUiLabels}
                tabLabels={referenceGroupLabels}
                onChange={handleComposerChange}
                onReady={handleComposerReady}
                onSubmit={handleSend}
                onFocus={onFocus}
                onPasteFiles={handlePasteFiles}
                onPlainPaste={handlePlainPasteShortcut}
                submitShortcut={shortcuts.send_message}
                newlineShortcut={shortcuts.newline_in_message}
                isExternalMenuOpen={slashMenuOpen && slashAutocompleteCount > 0}
                onExternalMenuKeyDown={handleExternalMenuKeyDown}
                partialText={voice.partialText}
                className={cn(
                  "min-h-0 flex-1",
                  showPlaceholderActivity &&
                    "iyw-claw-composer-placeholder-active"
                )}
              />
              <div className="flex shrink-0 items-center justify-between gap-1 px-2 pb-2">
                <div className="flex min-w-0 items-center gap-1">
                  <DropdownMenu onOpenChange={handleAddMenuOpenChange}>
                    <DropdownMenuTrigger asChild>
                      <Button
                        disabled={disabled}
                        variant="ghost"
                        size="icon-xs"
                        className="shrink-0 text-muted-foreground"
                        title={t("addActions")}
                        aria-label={t("addActions")}
                      >
                        <Plus className="size-4" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                      side="top"
                      align="start"
                      className="min-w-48"
                    >
                      {desktopMode ? (
                        <DropdownMenuItem
                          onClick={() => {
                            handlePickFiles().catch((error) => {
                              console.error(
                                "[MessageInput] pick files from menu failed:",
                                error
                              )
                            })
                          }}
                        >
                          <Paperclip className="size-4" />
                          {t("attachFiles")}
                        </DropdownMenuItem>
                      ) : (
                        <DropdownMenuItem
                          onClick={() => {
                            handleUploadLocalFiles().catch((error) => {
                              console.error(
                                "[MessageInput] upload local files failed:",
                                error
                              )
                            })
                          }}
                        >
                          <Upload className="size-4" />
                          {t("attachLocalUpload")}
                        </DropdownMenuItem>
                      )}
                      {!showNativePaperclip && (
                        <DropdownMenuItem
                          onClick={() => setServerFilePickerOpen(true)}
                        >
                          <FolderSearch className="size-4" />
                          {t("attachServerFile")}
                        </DropdownMenuItem>
                      )}
                      <DropdownMenuItem
                        onClick={() => setProjectReferenceOpen(true)}
                      >
                        <FolderSearch className="size-4" />
                        {t("projectReference.menuLabel")}
                      </DropdownMenuItem>
                      <DropdownMenuSub>
                        <DropdownMenuSubTrigger>
                          <MessageSquareText className="size-4" />
                          {t("quickMessages")}
                        </DropdownMenuSubTrigger>
                        <DropdownMenuSubContent
                          className="min-w-40 overflow-y-auto"
                          style={{
                            maxWidth: "min(20rem, calc(100vw - 1rem))",
                            maxHeight:
                              "min(32rem, var(--radix-dropdown-menu-content-available-height))",
                          }}
                        >
                          {quickMessagesLoading &&
                          quickMessages.length === 0 ? (
                            <div className="px-3 py-4 text-center text-xs text-muted-foreground">
                              {t("quickMessagesLoading")}
                            </div>
                          ) : quickMessages.length === 0 ? (
                            <div className="px-3 py-4 text-center text-xs text-muted-foreground">
                              {t("quickMessagesEmpty")}
                            </div>
                          ) : (
                            quickMessages.map((message) => (
                              <DropdownMenuItem
                                key={message.id}
                                onClick={() =>
                                  handleQuickMessageSelect(message)
                                }
                              >
                                <span className="truncate">
                                  {message.title || (
                                    <span className="italic text-muted-foreground">
                                      {t("quickMessageUntitled")}
                                    </span>
                                  )}
                                </span>
                              </DropdownMenuItem>
                            ))
                          )}
                        </DropdownMenuSubContent>
                      </DropdownMenuSub>
                      <TaskCommandMenu
                        commands={slashCommands}
                        skills={visibleEnabledSkills}
                        skillPrefix={skillPrefix}
                        onSelect={handleTaskCommandSelect}
                      />
                      <DropdownMenuSub
                        onOpenChange={handleSkillsMenuOpenChange}
                      >
                        <DropdownMenuSubTrigger disabled={!agentType}>
                          <BookOpenText className="size-4" />
                          {t("skills")}
                        </DropdownMenuSubTrigger>
                        <DropdownMenuSubContent
                          className="min-w-44 overflow-y-auto"
                          style={{
                            maxWidth: "min(20rem, calc(100vw - 1rem))",
                            maxHeight:
                              "min(32rem, var(--radix-dropdown-menu-content-available-height))",
                          }}
                        >
                          {skillsMenuScanning &&
                          visibleEnabledSkills.length === 0 ? (
                            <div className="px-3 py-4 text-center text-xs text-muted-foreground">
                              {t("skillsLoading")}
                            </div>
                          ) : skillsMenuScanFailed &&
                            visibleEnabledSkills.length === 0 ? (
                            <div className="px-3 py-4 text-center text-xs text-destructive">
                              {t("skillsScanFailed")}
                            </div>
                          ) : visibleEnabledSkills.length === 0 ? (
                            <div className="px-3 py-4 text-center text-xs text-muted-foreground">
                              {t("skillsEmpty")}
                            </div>
                          ) : (
                            visibleEnabledSkills.map((skill) => (
                              <DropdownMenuItem
                                key={`${skill.scope}-${skill.id}`}
                                onClick={() => handleSkillMenuSelect(skill)}
                              >
                                <BookOpenText className="size-4" />
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate">
                                    {skill.name || skill.id}
                                  </span>
                                  {skill.description && (
                                    <span className="block truncate text-xs text-muted-foreground">
                                      {skill.description}
                                    </span>
                                  )}
                                </span>
                              </DropdownMenuItem>
                            ))
                          )}
                        </DropdownMenuSubContent>
                      </DropdownMenuSub>
                      {/* A custom-dir pi can't have skills managed by iyw-claw's
                          default-dir store, so hide these shortcuts instead of
                          offering ones that lock with a Settings path the
                          Experts/Office matrices also hide for this agent. */}
                      {skillManagementSupported && (
                        <>
                          <DropdownMenuSub>
                            <DropdownMenuSubTrigger
                              disabled={expertsSorted.length === 0}
                            >
                              <Sparkles className="size-4" />
                              {t("experts")}
                            </DropdownMenuSubTrigger>
                            <DropdownMenuSubContent
                              className="min-w-44 overflow-y-auto"
                              style={{
                                maxWidth: "min(20rem, calc(100vw - 1rem))",
                                maxHeight:
                                  "min(32rem, var(--radix-dropdown-menu-content-available-height))",
                              }}
                            >
                              {expertsSorted.map((item) => {
                                const Icon = getExpertIcon(item.metadata.icon)
                                const label =
                                  pickLocalized(
                                    item.metadata.display_name,
                                    locale
                                  ) || item.metadata.id
                                return (
                                  <DropdownMenuItem
                                    key={item.metadata.id}
                                    onClick={() => handleExpertShortcut(item)}
                                  >
                                    <Icon className="size-4" />
                                    <span className="flex-1 truncate">
                                      {label}
                                    </span>
                                    {isSkillLocked(item.metadata.id) && (
                                      <Lock className="ml-auto size-3.5 shrink-0 text-muted-foreground/70" />
                                    )}
                                  </DropdownMenuItem>
                                )
                              })}
                            </DropdownMenuSubContent>
                          </DropdownMenuSub>
                          <DropdownMenuSub>
                            <DropdownMenuSubTrigger>
                              <FileStack className="size-4" />
                              {t("office")}
                            </DropdownMenuSubTrigger>
                            <DropdownMenuSubContent
                              className="min-w-44 overflow-y-auto"
                              style={{
                                maxWidth: "min(20rem, calc(100vw - 1rem))",
                                maxHeight:
                                  "min(32rem, var(--radix-dropdown-menu-content-available-height))",
                              }}
                            >
                              {OFFICE_ACTIONS.map((action) => {
                                const Icon = action.icon
                                const label = tQa(
                                  action.id as Parameters<typeof tQa>[0]
                                )
                                return (
                                  <DropdownMenuItem
                                    key={action.id}
                                    onClick={() => handleOfficeShortcut(action)}
                                  >
                                    <Icon className="size-4" />
                                    <span className="flex-1 truncate">
                                      {label}
                                    </span>
                                    {isSkillLocked(action.skillId) && (
                                      <Lock className="ml-auto size-3.5 shrink-0 text-muted-foreground/70" />
                                    )}
                                  </DropdownMenuItem>
                                )
                              })}
                            </DropdownMenuSubContent>
                          </DropdownMenuSub>
                        </>
                      )}
                    </DropdownMenuContent>
                  </DropdownMenu>
                  {hasInlineSelectors && (
                    <div className="hidden min-w-0 items-end gap-1 @[30rem]:flex">
                      {inlineSelectorItems}
                    </div>
                  )}
                  {hasAnySelector && (
                    <div
                      className={cn(
                        "flex",
                        hasInlineSelectors && "@[30rem]:hidden"
                      )}
                    >
                      <Popover
                        open={collapsedSelectorsOpen}
                        onOpenChange={setCollapsedSelectorsOpen}
                      >
                        <PopoverTrigger asChild>
                          <Button
                            variant="ghost"
                            size="icon-xs"
                            className="shrink-0"
                            title={t("agentSettings")}
                            aria-label={t("agentSettings")}
                          >
                            <Cog className="size-3" />
                          </Button>
                        </PopoverTrigger>
                        <PopoverContent
                          ref={collapsedSelectorsGuard.contentRef}
                          side="top"
                          align="start"
                          aria-label={t("agentSettings")}
                          onPointerDownOutside={
                            collapsedSelectorsGuard.onPointerDownOutside
                          }
                          onFocusOutside={
                            collapsedSelectorsGuard.onFocusOutside
                          }
                          className="w-[22rem] max-w-[calc(100vw-1rem)] p-1"
                        >
                          {showConfigLoading && (
                            <SelectorLoadingChip label={t("loadingSettings")} />
                          )}
                          {showModeLoading && (
                            <SelectorLoadingChip label={t("loadingMode")} />
                          )}
                          {collapsedSettings.length > 0 && (
                            <SessionSelectorsPanel
                              settings={collapsedSettings}
                              settingsLabel={t("agentSettings")}
                              onAfterSelect={() =>
                                setCollapsedSelectorsOpen(false)
                              }
                            />
                          )}
                        </PopoverContent>
                      </Popover>
                    </div>
                  )}
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <SessionUsageChip
                    contextKey={attachmentTabId ?? null}
                    popoverSide="top"
                    popoverSideOffset={8}
                  />
                  {desktopMode && !isEditingQueueItem && (
                    <RealtimeVoiceButton
                      status={voice.status}
                      autoSend={voice.autoSend}
                      disabled={
                        voice.status === "starting" ||
                        voice.status === "stopping" ||
                        (voice.status === "idle" && disabled)
                      }
                      onToggle={voice.toggle}
                      onAutoSendChange={voice.setAutoSend}
                    />
                  )}
                  {actionButtons}
                </div>
              </div>
              {showDragActive && (
                <div className="pointer-events-none absolute inset-1 z-20 flex items-center justify-center rounded-md border border-dashed border-primary/50 bg-background/80 text-xs text-muted-foreground">
                  {t("dropFilesToAttach")}
                </div>
              )}
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem
              disabled={disabled || !contextSelectionActive}
              onSelect={() => void handleContextCut()}
            >
              <Scissors className="size-4" />
              {t("cut")}
            </ContextMenuItem>
            <ContextMenuItem
              disabled={!contextSelectionActive}
              onSelect={() => void handleContextCopy()}
            >
              <Copy className="size-4" />
              {t("copy")}
            </ContextMenuItem>
            <ContextMenuItem
              disabled={disabled}
              onSelect={() => {
                void handleContextPaste()
              }}
            >
              <ClipboardPaste className="size-4" />
              {t("pasteAsPlainText")}
            </ContextMenuItem>
            <ContextMenuItem
              disabled={disabled}
              onSelect={() => handleContextSelectAll()}
            >
              <TextSelect className="size-4" />
              {t("selectAll")}
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuSub>
              <ContextMenuSubTrigger disabled={disabled}>
                <MessageSquareText className="size-4" />
                {t("quickMessages")}
              </ContextMenuSubTrigger>
              <ContextMenuSubContent
                className="min-w-40 overflow-y-auto"
                style={{
                  maxWidth: "min(20rem, calc(100vw - 1rem))",
                  maxHeight:
                    "min(32rem, var(--radix-context-menu-content-available-height))",
                }}
              >
                {quickMessagesLoading && quickMessages.length === 0 ? (
                  <div className="px-3 py-4 text-center text-xs text-muted-foreground">
                    {t("quickMessagesLoading")}
                  </div>
                ) : quickMessages.length === 0 ? (
                  <div className="px-3 py-4 text-center text-xs text-muted-foreground">
                    {t("quickMessagesEmpty")}
                  </div>
                ) : (
                  quickMessages.map((message) => (
                    <ContextMenuItem
                      key={message.id}
                      onSelect={() => handleQuickMessageSelect(message)}
                    >
                      <span className="truncate">
                        {message.title || (
                          <span className="italic text-muted-foreground">
                            {t("quickMessageUntitled")}
                          </span>
                        )}
                      </span>
                    </ContextMenuItem>
                  ))
                )}
              </ContextMenuSubContent>
            </ContextMenuSub>
          </ContextMenuContent>
        </ContextMenu>
        {hasFolderBranchPicker && (
          // `pl-2` mirrors the action bar's `px-2` so this row lines up with the
          // composer above. Kept on the rem scale (no px literals) so it tracks
          // UI zoom; the folder icon then aligns with the centered "+" icon
          // because both buttons add the same 1px transparent border (paired
          // with the picker buttons' `px-1.5`).
          <div
            className={cn(
              "flex items-center gap-1 pl-2 text-xs text-muted-foreground",
              folderBranchPickerAttached ? "rounded-b-xl pt-1 pr-2" : "mt-1.5"
            )}
          >
            <ConversationFolderBranchPicker tabId={attachmentTabId} />
          </div>
        )}
      </div>
      <ImagePreviewDialog
        src={previewAttachmentSrc}
        alt={previewAttachment?.name ?? ""}
        open={previewAttachment !== null && previewAttachmentSrc.length > 0}
        onOpenChange={(open) => {
          if (!open) setPreviewAttachmentId(null)
        }}
        navigation={
          previewAttachmentIndex >= 0
            ? {
                index: previewAttachmentIndex,
                total: imageAttachments.length,
                onIndexChange: (index) => {
                  setPreviewAttachmentId(imageAttachments[index]?.id ?? null)
                },
              }
            : undefined
        }
        onApply={previewAttachment ? applyAttachmentEdit : undefined}
      />
      {!showNativePaperclip && (
        <ServerFileBrowserDialog
          open={serverFilePickerOpen}
          onOpenChange={setServerFilePickerOpen}
          onSelect={handleServerFilesSelected}
          initialPath={defaultPath ?? undefined}
        />
      )}
      <ProjectReferenceDialog
        open={projectReferenceOpen}
        onOpenChange={setProjectReferenceOpen}
        rootPath={defaultPath ?? null}
        onSelect={handleProjectReferenceSelect}
        onBrowseFolder={handlePickFolder}
      />
    </div>
  )
}
