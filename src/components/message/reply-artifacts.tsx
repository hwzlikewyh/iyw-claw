"use client"

import { memo, useMemo, useState } from "react"
import { ChevronRight, FileDiff, FileIcon } from "lucide-react"
import { useTranslations } from "next-intl"
import { useActiveFolder } from "@/contexts/active-folder-context"
import {
  CommitFileAdditions,
  CommitFileDeletions,
} from "@/components/ai-elements/commit"
import { UnifiedDiffPreview } from "@/components/diff/unified-diff-preview"
import {
  fileNameOf,
  isAddedFileDiff,
  isRemovedFileDiff,
  toFolderRelativePath,
} from "@/lib/file-path-display"
import { extractReplyFileChanges } from "@/lib/session-files"
import type { MessageTurn } from "@/lib/types"
import { cn } from "@/lib/utils"

/**
 * Inline "artifacts" card shown at the end of a completed assistant reply
 * (above the `TurnStats` action row inside `HistoricalMessageGroup`).
 *
 * Modified/removed files render as a single-open accordion (only one diff
 * expanded at a time). Newly created files stay visible in their tool calls
 * and the workspace, but are intentionally omitted from this reply summary.
 *
 * Diffs are parsed lazily and ONLY once the reply is persisted
 * (`isResponseComplete`), so the streaming hot path never runs diff parsing.
 */
export const ReplyArtifacts = memo(function ReplyArtifacts({
  sourceTurns,
  isResponseComplete,
}: {
  sourceTurns: MessageTurn[]
  isResponseComplete: boolean
}) {
  const t = useTranslations("Folder.chat.replyArtifacts")
  const { activeFolder: folder } = useActiveFolder()
  const [changedOpen, setChangedOpen] = useState(false)
  // Single-open accordion: the path of the one changed file whose diff is open.
  const [openPath, setOpenPath] = useState<string | null>(null)

  // Guard parsing behind completion so mid-stream renders stay diff-free.
  const files = useMemo(
    () => (isResponseComplete ? extractReplyFileChanges(sourceTurns) : []),
    [isResponseComplete, sourceTurns]
  )

  const changedFiles = useMemo(
    () =>
      files.filter(
        (file) => isRemovedFileDiff(file.diff) || !isAddedFileDiff(file.diff)
      ),
    [files]
  )

  if (!isResponseComplete) return null
  if (changedFiles.length === 0) return null

  const folderPath = folder?.path

  const totalAdditions = changedFiles.reduce((sum, f) => sum + f.additions, 0)
  const totalDeletions = changedFiles.reduce((sum, f) => sum + f.deletions, 0)

  return (
    <div className="mt-2 space-y-2">
      {changedFiles.length > 0 && (
        <div className="overflow-hidden rounded-lg border border-border bg-card/40 text-card-foreground">
          <button
            type="button"
            aria-expanded={changedOpen}
            onClick={() => setChangedOpen((prev) => !prev)}
            className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
          >
            <FileDiff className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className="text-xs font-medium text-foreground">
              {t("title")}
            </span>
            <span className="rounded-md border border-border bg-muted/40 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {t("fileCount", { count: changedFiles.length })}
            </span>
            {/* Always render BOTH counts (incl. zeros) so a one-sided reply
                still shows its +N and -N. */}
            <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-muted/40 px-1.5 py-0.5 font-mono text-[10px]">
              <span className="text-green-600 dark:text-green-400">
                +{totalAdditions}
              </span>
              <span className="text-red-600 dark:text-red-400">
                -{totalDeletions}
              </span>
            </span>
            <ChevronRight
              className={cn(
                "ms-auto h-4 w-4 shrink-0 text-muted-foreground transition-transform",
                changedOpen && "rotate-90"
              )}
            />
          </button>

          {changedOpen && (
            <ul className="max-h-80 space-y-1.5 overflow-y-auto border-t border-border p-2">
              {changedFiles.map((file) => {
                const displayPath = toFolderRelativePath(file.path, folderPath)
                const name = fileNameOf(displayPath)
                const dir =
                  displayPath === name
                    ? ""
                    : displayPath.slice(0, displayPath.length - name.length - 1)
                const isRemoved = isRemovedFileDiff(file.diff)
                const isOpen = openPath === file.path

                return (
                  <li
                    key={file.id}
                    className={cn(
                      "overflow-hidden rounded-md border transition-colors",
                      isRemoved ? "border-destructive/30" : "border-border",
                      isOpen && "bg-muted/20"
                    )}
                  >
                    <button
                      type="button"
                      aria-expanded={isOpen}
                      onClick={() => setOpenPath(isOpen ? null : file.path)}
                      title={displayPath}
                      className={cn(
                        "flex w-full min-w-0 items-center gap-2 px-2 py-1.5 text-left transition-colors",
                        isRemoved
                          ? "hover:bg-destructive/10"
                          : "hover:bg-accent/40",
                        isOpen &&
                          (isRemoved
                            ? "border-b border-destructive/30"
                            : "border-b border-border")
                      )}
                    >
                      <ChevronRight
                        className={cn(
                          "h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform",
                          isOpen && "rotate-90"
                        )}
                      />
                      <FileIcon
                        className={cn(
                          "h-3.5 w-3.5 shrink-0",
                          isRemoved
                            ? "text-destructive"
                            : "text-muted-foreground"
                        )}
                      />
                      <span className="flex min-w-0 flex-1 items-baseline gap-2">
                        <span
                          className={cn(
                            "min-w-0 truncate text-xs",
                            isRemoved ? "text-destructive" : "text-foreground"
                          )}
                        >
                          {name}
                        </span>
                        {dir && (
                          <span className="min-w-0 flex-1 truncate text-[10px] text-muted-foreground">
                            {dir}
                          </span>
                        )}
                      </span>
                      {isRemoved ? (
                        <span className="inline-flex shrink-0 items-center rounded-md border border-destructive/30 bg-destructive/10 px-1.5 py-0.5 font-mono text-[10px] text-destructive">
                          {t("remove")}
                        </span>
                      ) : (
                        <span className="inline-flex shrink-0 items-center gap-1 rounded-md border border-border bg-muted/40 px-1.5 py-0.5 font-mono text-[10px] text-foreground">
                          <CommitFileAdditions
                            count={file.additions}
                            className="text-[10px]"
                          />
                          <CommitFileDeletions
                            count={file.deletions}
                            className="text-[10px]"
                          />
                        </span>
                      )}
                    </button>

                    {isOpen &&
                      (file.diff ? (
                        <UnifiedDiffPreview diffText={file.diff} embedded />
                      ) : (
                        <p className="px-3 py-2 text-xs text-muted-foreground">
                          {t("noDiffDataAvailable", { filePath: displayPath })}
                        </p>
                      ))}
                  </li>
                )
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  )
})
