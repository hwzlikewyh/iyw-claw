"use client"

import { useCallback, useMemo, useRef, useState } from "react"
import {
  ChevronDown,
  ChevronRight,
  File,
  Folder,
  FolderOpen,
  RotateCcw,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Virtualizer } from "virtua"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import type { SkillMarketV2FileNode } from "@/lib/skill-market"
import { formatSkillBytes } from "@/lib/skill-market"

const ROW_HEIGHT = 24
const VIRTUALIZE_THRESHOLD = 200

interface FlattenedNode {
  node: SkillMarketV2FileNode
  depth: number
}

function flattenTree(
  nodes: SkillMarketV2FileNode[],
  expanded: ReadonlySet<string>
): FlattenedNode[] {
  const result: FlattenedNode[] = []
  const walk = (items: SkillMarketV2FileNode[], depth: number) => {
    for (const node of items) {
      result.push({ node, depth })
      if (node.directory && node.children && expanded.has(node.path)) {
        walk(node.children, depth + 1)
      }
    }
  }
  walk(nodes, 0)
  return result
}

function countFiles(nodes: SkillMarketV2FileNode[]): number {
  let count = 0
  for (const node of nodes) {
    if (node.directory) count += countFiles(node.children ?? [])
    else count += 1
  }
  return count
}

function allDirectoryPaths(nodes: SkillMarketV2FileNode[]): string[] {
  const paths: string[] = []
  for (const node of nodes) {
    if (node.directory) {
      paths.push(node.path)
      paths.push(...allDirectoryPaths(node.children ?? []))
    }
  }
  return paths
}

function FileRow({
  row,
  expanded,
  onToggle,
}: {
  row: FlattenedNode
  expanded: boolean
  onToggle: (path: string) => void
}) {
  const t = useTranslations("SkillMarketV2")
  const { node, depth } = row
  const isDirectory = node.directory
  return (
    <div
      className="flex h-6 items-center gap-1.5 pr-2 text-xs"
      style={{ paddingLeft: `${8 + depth * 16}px` }}
    >
      {isDirectory ? (
        <Button
          size="icon-xs"
          variant="ghost"
          className="size-4 shrink-0"
          aria-label={
            expanded ? t("a11y.collapseDir") : t("a11y.expandDir")
          }
          onClick={() => onToggle(node.path)}
        >
          {expanded ? (
            <ChevronDown className="size-3" aria-hidden="true" />
          ) : (
            <ChevronRight className="size-3" aria-hidden="true" />
          )}
        </Button>
      ) : (
        <span className="size-4 shrink-0" aria-hidden="true" />
      )}
      {isDirectory ? (
        expanded ? (
          <FolderOpen className="size-3.5 shrink-0 text-amber-500" aria-hidden="true" />
        ) : (
          <Folder className="size-3.5 shrink-0 text-amber-500" aria-hidden="true" />
        )
      ) : (
        <File className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      )}
      {isDirectory ? (
        <button
          type="button"
          className="truncate rounded-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          onClick={() => onToggle(node.path)}
        >
          {node.name}
        </button>
      ) : (
        <span
          className="truncate font-mono"
          title={node.path}
        >
          {node.name}
        </span>
      )}
      {!isDirectory ? (
        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">
          {formatSkillBytes(node.size)}
        </span>
      ) : null}
    </div>
  )
}

export interface SkillMarketFilesTreeProps {
  files: SkillMarketV2FileNode[]
  loading: boolean
  error: string | null
  onRetry: () => void
}

export function SkillMarketFilesTree(props: SkillMarketFilesTreeProps) {
  const t = useTranslations("SkillMarketV2")
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(
    () => new Set()
  )
  const [viewport, setViewport] = useState<HTMLElement | null>(null)
  const viewportRef = useRef<HTMLElement | null>(null)
  const handleViewportRef = useCallback((element: HTMLElement | null) => {
    viewportRef.current = element
    setViewport(element)
  }, [])
  const rows = useMemo(
    () => flattenTree(props.files, expanded),
    [expanded, props.files]
  )
  const fileCount = useMemo(() => countFiles(props.files), [props.files])

  const toggle = (path: string) => {
    setExpanded((current) => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }
  const expandAll = () => {
    setExpanded(new Set(allDirectoryPaths(props.files)))
  }
  const collapseAll = () => {
    setExpanded(new Set())
  }

  if (props.loading) {
    return (
      <div className="rounded-md border p-2 text-xs text-muted-foreground">
        {t("detail.filesLoading")}
      </div>
    )
  }
  if (props.error) {
    return (
      <div className="flex items-center gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-2.5 py-2 text-xs text-destructive">
        <span className="min-w-0 flex-1 break-words">{props.error}</span>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={t("detail.retry")}
          title={t("detail.retry")}
          onClick={props.onRetry}
        >
          <RotateCcw className="size-3.5" aria-hidden="true" />
        </Button>
      </div>
    )
  }
  if (!rows.length) {
    return (
      <div className="rounded-md border border-dashed p-3 text-xs text-muted-foreground">
        {t("detail.filesEmpty")}
      </div>
    )
  }

  return (
    <div className="min-h-0">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">
          {t("detail.fileCount", { count: fileCount })}
        </span>
        <div className="flex shrink-0 gap-1">
          <Button size="xs" variant="ghost" onClick={expandAll}>
            {t("detail.expandAll")}
          </Button>
          <Button size="xs" variant="ghost" onClick={collapseAll}>
            {t("detail.collapseAll")}
          </Button>
        </div>
      </div>
      <div className="mt-1 overflow-hidden rounded-md border">
        <ScrollArea className="h-72" onViewportRef={handleViewportRef}>
          {viewport && rows.length > VIRTUALIZE_THRESHOLD ? (
            <Virtualizer
              data={rows}
              itemSize={ROW_HEIGHT}
              bufferSize={400}
              scrollRef={viewportRef}
            >
              {(row) => (
                <FileRow
                  key={row.node.path}
                  row={row}
                  expanded={expanded.has(row.node.path)}
                  onToggle={toggle}
                />
              )}
            </Virtualizer>
          ) : (
            rows.map((row) => (
              <FileRow
                key={row.node.path}
                row={row}
                expanded={expanded.has(row.node.path)}
                onToggle={toggle}
              />
            ))
          )}
        </ScrollArea>
      </div>
    </div>
  )
}
