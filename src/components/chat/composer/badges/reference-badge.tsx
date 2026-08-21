import {
  Archive,
  BookOpenCheck,
  Bot,
  Command,
  File,
  FileCode,
  FileImage,
  FileSpreadsheet,
  FileText,
  Folder,
  Focus,
  GitCommit,
  Hash,
  MessageSquare,
  RefreshCw,
  Sparkles,
} from "lucide-react"
import type { ReactNode } from "react"

import { AgentIcon } from "@/components/agent-icon"
import { isAgentType } from "@/lib/types"
import { cn } from "@/lib/utils"

import type { ReferenceAttrs } from "../types"
import { isTaskReference } from "../composer-commands"

const ICON_CLASS = "size-3.5 shrink-0"

const SPREADSHEET_EXTENSIONS = new Set(["csv", "ods", "xls", "xlsx"])
const ARCHIVE_EXTENSIONS = new Set([
  "7z",
  "bz2",
  "gz",
  "rar",
  "tar",
  "xz",
  "zip",
])
const IMAGE_EXTENSIONS = new Set(["gif", "jpeg", "jpg", "png", "webp"])
const CODE_EXTENSIONS = new Set([
  "css",
  "go",
  "html",
  "java",
  "js",
  "json",
  "jsx",
  "py",
  "rs",
  "sh",
  "sql",
  "ts",
  "tsx",
  "vue",
  "xml",
  "yaml",
  "yml",
])
const TEXT_EXTENSIONS = new Set(["doc", "docx", "md", "pdf", "rtf", "txt"])

function fileIcon(label: string): ReactNode {
  const plainLabel = label.replace(/:\d+(?:-\d+)?$/, "")
  const extension = plainLabel.split(".").pop()?.toLowerCase() ?? ""
  if (SPREADSHEET_EXTENSIONS.has(extension)) {
    return <FileSpreadsheet className={ICON_CLASS} />
  }
  if (ARCHIVE_EXTENSIONS.has(extension)) {
    return <Archive className={ICON_CLASS} />
  }
  if (IMAGE_EXTENSIONS.has(extension))
    return <FileImage className={ICON_CLASS} />
  if (CODE_EXTENSIONS.has(extension)) return <FileCode className={ICON_CLASS} />
  if (TEXT_EXTENSIONS.has(extension)) return <FileText className={ICON_CLASS} />
  return <File className={ICON_CLASS} />
}

export function ReferenceIcon({
  data,
  variant = "badge",
}: {
  data: ReferenceAttrs
  /**
   * Where the icon is shown. `"badge"` (default) is the inline reference chip in
   * the composer and the message transcript; `"option"` is a row in the `@`
   * panel. They differ only for sessions (see the `session` case).
   */
  variant?: "badge" | "option"
}) {
  const meta = data.meta
  let icon: ReactNode = null
  switch (data.refType) {
    case "file":
      icon =
        meta?.fileKind === "dir" ? (
          <Folder className={ICON_CLASS} />
        ) : (
          fileIcon(data.label || data.id)
        )
      break
    case "agent": {
      const agentType =
        meta?.agentType ?? (isAgentType(data.id) ? data.id : null)
      icon = agentType ? (
        <AgentIcon agentType={agentType} className={ICON_CLASS} />
      ) : (
        <Bot className={ICON_CLASS} />
      )
      break
    }
    case "session":
      // The inline badge (composer + transcript) shows a neutral conversation
      // glyph: a session reference reads as "a conversation", not as the agent
      // that owns it, and it carries no live status. The `@`-panel option row
      // (`variant="option"`) instead shows the owning agent's icon so sessions
      // stay distinguishable while picking one (falling back to `Hash` for a
      // legacy id with no recoverable agent type).
      icon =
        variant === "option" ? (
          meta?.agentType ? (
            <AgentIcon agentType={meta.agentType} className={ICON_CLASS} />
          ) : (
            <Hash className={ICON_CLASS} />
          )
        ) : (
          <MessageSquare className={ICON_CLASS} />
        )
      break
    case "commit":
      icon = <GitCommit className={ICON_CLASS} />
      break
    case "skill":
      if (isTaskReference(data)) {
        icon =
          data.id
            .trim()
            .replace(/^[/$]+/, "")
            .toLowerCase() === "loop" ? (
            <RefreshCw className={ICON_CLASS} />
          ) : (
            <Focus className={ICON_CLASS} />
          )
      } else if (meta?.scope === "expert") {
        icon = <Sparkles className={ICON_CLASS} />
      } else if (meta?.scope != null) {
        icon = <BookOpenCheck className={ICON_CLASS} />
      } else {
        icon = <Command className={ICON_CLASS} />
      }
      break
    default:
      return null
  }
  // Decorative wherever it appears (popup option, badge): the accessible name
  // comes from the adjacent label (or the badge's own role="img" name), so hide
  // it — otherwise AgentIcon's titled <svg> leaks into the option name (e.g.
  // "Codex Codex Helper").
  return (
    <span aria-hidden="true" className="inline-flex shrink-0">
      {icon}
    </span>
  )
}

/**
 * Per-kind low-saturation surface treatment. The border keeps inline references
 * legible on both user and assistant message backgrounds without turning them
 * into large pills.
 */
function badgeColorClass(data: ReferenceAttrs): string {
  switch (data.refType) {
    case "file":
      return "border-border/70 bg-muted/45 text-foreground/80"
    case "agent":
      return "border-cyan-500/25 bg-cyan-500/[0.07] text-cyan-800 dark:text-cyan-300"
    case "session":
      return "border-emerald-500/25 bg-emerald-500/[0.07] text-emerald-800 dark:text-emerald-300"
    case "commit":
      return "border-amber-500/25 bg-amber-500/[0.07] text-amber-800 dark:text-amber-300"
    case "skill":
      if (isTaskReference(data)) {
        return "border-sky-500/25 bg-sky-500/[0.07] text-sky-800 dark:text-sky-300"
      }
      if (data.meta?.scope === "expert") {
        return "border-amber-500/25 bg-amber-500/[0.07] text-amber-800 dark:text-amber-300"
      }
      if (data.meta?.scope != null) {
        return "border-violet-500/25 bg-violet-500/[0.07] text-violet-800 dark:text-violet-300"
      }
      return "border-border/70 bg-muted/45 text-foreground/80"
  }
}

function referenceCategory(data: ReferenceAttrs): string {
  if (data.refType !== "skill") return data.refType
  if (isTaskReference(data)) return "task"
  if (data.meta?.scope === "expert") return "expert"
  return data.meta?.scope != null ? "skill" : "command"
}

export interface ReferenceBadgeProps {
  data: ReferenceAttrs
  className?: string
}

/**
 * Presentational inline chip for a reference. Shared by the editor node view and
 * the message-transcript rendering (markdown-link → here). Purely visual — no
 * editor coupling.
 */
export function ReferenceBadge({ data, className }: ReferenceBadgeProps) {
  const label =
    data.refType === "file" ? data.label || "file" : data.label || data.id
  return (
    <span
      data-reference-badge=""
      data-ref-type={data.refType}
      title={label}
      // The badge is an inline contentEditable=false atom. `role="img"` makes it
      // a single named unit so `aria-label` is a reliable accessible name (a
      // bare span's aria-label is not), and collapses the decorative icon —
      // including AgentIcon's titled <svg> — into that one name.
      role="img"
      aria-label={`${referenceCategory(data)}: ${label}`}
      className={cn(
        "inline-flex max-w-[18rem] items-center gap-1 rounded-[4px] border px-1.5 py-0.5 align-middle text-[0.82em] font-medium leading-none",
        badgeColorClass(data),
        className
      )}
    >
      <ReferenceIcon data={data} />
      <span className="truncate">{label}</span>
    </span>
  )
}
