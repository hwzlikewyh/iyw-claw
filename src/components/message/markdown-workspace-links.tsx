"use client"

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type ReactNode,
} from "react"
import {
  defaultRehypePlugins,
  defaultRemarkPlugins,
  type Components,
} from "streamdown"

import { MarkdownLink } from "@/components/ai-elements/markdown-link"
import { rehypePluginsAllowingIywClaw } from "@/components/ai-elements/rehype-allow-iyw-claw"
import {
  parseWorkspaceFileLinkUri,
  resolvedWorkspaceLinksPlugin,
  resolveMarkdownWorkspaceLinks,
  type LinkPresentation,
  type ResolvedLink,
  type WorkspaceRoot,
} from "@/components/message/markdown-workspace-link-resolver"
import { useWorkspaceActions } from "@/contexts/workspace-context"
import { cn } from "@/lib/utils"

interface ResolutionState {
  content: string
  documentPath: string
  rootsKey: string
  links: Map<number, ResolvedLink>
}

const EMPTY_LINKS = new Map<number, ResolvedLink>()

export function useWorkspaceMarkdownOptions({
  content,
  documentPath,
  roots,
  onOpenWorkspace,
}: {
  content: string
  documentPath: string
  roots: WorkspaceRoot[]
  onOpenWorkspace?: () => void
}) {
  const [resolved, setResolved] = useState<ResolutionState | null>(null)
  const rootsKey = roots.map((root) => `${root.id}:${root.path}`).join("\0")
  const links =
    resolved?.content === content &&
    resolved.documentPath === documentPath &&
    resolved.rootsKey === rootsKey
      ? resolved.links
      : EMPTY_LINKS
  useEffect(() => {
    let active = true
    void resolveMarkdownWorkspaceLinks(content, documentPath, roots).then(
      (next) => {
        if (active) {
          setResolved({ content, documentPath, rootsKey, links: next })
        }
      }
    )
    return () => {
      active = false
    }
  }, [content, documentPath, roots, rootsKey])

  const remarkPlugins = useMemo(
    () => [
      ...Object.values(defaultRemarkPlugins),
      resolvedWorkspaceLinksPlugin(links),
    ],
    [links]
  )
  const allowedLinks = useMemo(
    () => new Set(Array.from(links.values(), workspaceLinkKey)),
    [links]
  )
  const components = useMemo<Components>(
    () => ({
      a: (props) => (
        <WorkspaceMarkdownLink
          {...props}
          allowedLinks={allowedLinks}
          onOpenWorkspace={onOpenWorkspace}
        />
      ),
    }),
    [allowedLinks, onOpenWorkspace]
  )
  return { components, remarkPlugins, rehypePlugins }
}

const rehypePlugins = rehypePluginsAllowingIywClaw(defaultRehypePlugins)

function WorkspaceMarkdownLink({
  href,
  children,
  allowedLinks,
  onOpenWorkspace,
  ...props
}: ComponentProps<"a"> & {
  node?: unknown
  allowedLinks: Set<string>
  onOpenWorkspace?: () => void
}) {
  const resolved = parseWorkspaceFileLinkUri(href)
  if (!resolved || !allowedLinks.has(workspaceLinkKey(resolved))) {
    return (
      <MarkdownLink href={href} {...props}>
        {children}
      </MarkdownLink>
    )
  }
  return (
    <WorkspaceFileLink
      target={resolved.path}
      presentation={resolved.presentation}
      onOpenWorkspace={onOpenWorkspace}
    >
      {children}
    </WorkspaceFileLink>
  )
}

function workspaceLinkKey(link: ResolvedLink): string {
  return `${link.presentation}\0${link.path}`
}

function WorkspaceFileLink({
  target,
  presentation,
  onOpenWorkspace,
  children,
}: {
  target: string
  presentation: LinkPresentation
  onOpenWorkspace?: () => void
  children: ReactNode
}) {
  const { openFilePreview } = useWorkspaceActions()
  const [opening, setOpening] = useState(false)
  const openingRef = useRef(false)
  const open = useCallback(() => {
    if (openingRef.current) return
    openingRef.current = true
    setOpening(true)
    onOpenWorkspace?.()
    void openFilePreview(target)
      .catch((error) => {
        console.error("[task-artifact-preview] workspace link open failed", {
          errorType: error instanceof Error ? error.name : typeof error,
        })
      })
      .finally(() => {
        openingRef.current = false
        setOpening(false)
      })
  }, [onOpenWorkspace, openFilePreview, target])
  return (
    <button
      type="button"
      title={target}
      aria-busy={opening}
      disabled={opening}
      onClick={open}
      className={cn(
        "wrap-anywhere cursor-pointer appearance-none text-left font-medium text-primary underline hover:opacity-80 disabled:cursor-wait disabled:opacity-70",
        presentation === "inline-code" &&
          "rounded bg-muted px-1.5 py-0.5 font-mono text-sm"
      )}
    >
      {children}
    </button>
  )
}
