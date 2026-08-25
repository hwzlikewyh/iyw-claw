import { fromMarkdown } from "mdast-util-from-markdown"

import { workspaceFileExists } from "@/lib/api"
import {
  findOwningFolder,
  joinRootRel,
  normalizeAbsPath,
} from "@/lib/file-open-target"
import { isAbsoluteFilePath, normalizeSlashPath } from "@/lib/file-path-display"

const WORKSPACE_LINK_PREFIX = "iyw-claw://workspace-file/"
const MAX_PATH_LENGTH = 512
const MAX_CANDIDATES = 64

export type LinkPresentation = "text" | "inline-code"

interface MarkdownNode {
  type: string
  value?: unknown
  url?: unknown
  children?: MarkdownNode[]
  position?: { start?: { offset?: number } }
}

export interface ResolvedLink {
  path: string
  presentation: LinkPresentation
}

interface LinkCandidate {
  offset: number
  path: string
  presentation: LinkPresentation
}

export interface WorkspaceRoot {
  id: number
  path: string
}

function decodePath(value: string): string {
  try {
    return decodeURIComponent(value)
  } catch {
    return value
  }
}

function pathCandidate(value: string, inlineCode: boolean): string | null {
  const trimmed = value.trim()
  if (
    !trimmed ||
    trimmed.length > MAX_PATH_LENGTH ||
    /[\r\n\0]/.test(trimmed)
  ) {
    return null
  }
  if (/^[a-z][a-z\d+.-]*:/i.test(trimmed) && !/^[a-z]:[\\/]/i.test(trimmed)) {
    return null
  }
  if (trimmed.startsWith("#") || trimmed.startsWith("//")) return null

  const boundary = trimmed.search(/[?#]/)
  const rawPath = boundary >= 0 ? trimmed.slice(0, boundary) : trimmed
  const decoded = normalizeSlashPath(decodePath(rawPath))
  if (!decoded || decoded.endsWith("/")) return null
  if (inlineCode && !decoded.includes("/") && !/\.[^/.\s]+$/.test(decoded)) {
    return null
  }
  return decoded
}

function collectCandidates(content: string): LinkCandidate[] {
  let tree: MarkdownNode
  try {
    tree = fromMarkdown(content) as MarkdownNode
  } catch {
    return []
  }

  const candidates: LinkCandidate[] = []
  const visit = (node: MarkdownNode) => {
    const offset = node.position?.start?.offset
    const inlineCode = node.type === "inlineCode"
    const raw = inlineCode ? node.value : node.type === "link" ? node.url : null
    if (typeof offset === "number" && typeof raw === "string") {
      const path = pathCandidate(raw, inlineCode)
      if (path) {
        candidates.push({
          offset,
          path,
          presentation: inlineCode ? "inline-code" : "text",
        })
      }
    }
    for (const child of node.children ?? []) visit(child)
  }
  visit(tree)
  return candidates.slice(0, MAX_CANDIDATES)
}

export function workspaceFileLinkUri(link: ResolvedLink): string {
  return `${WORKSPACE_LINK_PREFIX}${encodeURIComponent(link.path)}?presentation=${link.presentation}`
}

export function parseWorkspaceFileLinkUri(
  value: string | undefined
): ResolvedLink | null {
  if (!value?.toLowerCase().startsWith(WORKSPACE_LINK_PREFIX)) return null
  const payload = value.slice(WORKSPACE_LINK_PREFIX.length)
  const queryIndex = payload.indexOf("?")
  const encodedPath = queryIndex >= 0 ? payload.slice(0, queryIndex) : payload
  const query = queryIndex >= 0 ? payload.slice(queryIndex + 1) : ""
  try {
    return {
      path: decodeURIComponent(encodedPath),
      presentation:
        new URLSearchParams(query).get("presentation") === "inline-code"
          ? "inline-code"
          : "text",
    }
  } catch {
    return null
  }
}

async function existingFile(rootPath: string, path: string): Promise<boolean> {
  return workspaceFileExists(rootPath, path).catch(() => false)
}

async function resolveAbsolute(
  path: string,
  roots: WorkspaceRoot[]
): Promise<string | null> {
  const owning = findOwningFolder(path, roots)
  if (!owning) return null
  return (await existingFile(owning.rootPath, owning.relPath))
    ? joinRootRel(owning.rootPath, owning.relPath)
    : null
}

async function resolveFromRoots(
  path: string,
  roots: WorkspaceRoot[]
): Promise<string | null> {
  const relative = path.replace(/^\.?\/+/, "")
  const checks = roots.map(async (root) => {
    const absolute = joinRootRel(root.path, relative)
    const owning = findOwningFolder(absolute, [root])
    if (!owning || !(await existingFile(root.path, owning.relPath))) return null
    return absolute
  })
  const matches = Array.from(
    new Set(
      (await Promise.all(checks)).filter((path): path is string => !!path)
    )
  )
  return matches.length === 1 ? matches[0] : null
}

async function resolveCandidate(
  path: string,
  documentPath: string,
  roots: WorkspaceRoot[]
): Promise<string | null> {
  if (isAbsoluteFilePath(path) && !path.startsWith("/")) {
    return resolveAbsolute(normalizeAbsPath(path), roots)
  }
  if (path.startsWith("//")) {
    return resolveAbsolute(normalizeAbsPath(path), roots)
  }
  if (path.startsWith("/")) {
    const absolute = await resolveAbsolute(normalizeAbsPath(path), roots)
    return absolute ?? resolveFromRoots(path, roots)
  }

  const documentDir = documentPath.replace(/\/[^/]*$/, "")
  const documentRelative = normalizeAbsPath(`${documentDir}/${path}`)
  const fromDocument = await resolveAbsolute(documentRelative, roots)
  return fromDocument ?? resolveFromRoots(path, roots)
}

export async function resolveMarkdownWorkspaceLinks(
  content: string,
  documentPath: string,
  roots: WorkspaceRoot[]
): Promise<Map<number, ResolvedLink>> {
  const links = new Map<number, ResolvedLink>()
  if (!documentPath || roots.length === 0) return links

  const resolutions = new Map<string, Promise<string | null>>()
  const resolveOnce = (path: string) => {
    let pending = resolutions.get(path)
    if (!pending) {
      pending = resolveCandidate(path, documentPath, roots)
      resolutions.set(path, pending)
    }
    return pending
  }
  const results = await Promise.all(
    collectCandidates(content).map(async (candidate) => ({
      candidate,
      resolved: await resolveOnce(candidate.path),
    }))
  )
  for (const { candidate, resolved } of results) {
    if (resolved) {
      links.set(candidate.offset, {
        path: resolved,
        presentation: candidate.presentation,
      })
    }
  }
  return links
}

export function resolvedWorkspaceLinksPlugin(links: Map<number, ResolvedLink>) {
  return () => (tree: MarkdownNode) => {
    const visit = (node: MarkdownNode) => {
      const offset = node.position?.start?.offset
      const resolved =
        typeof offset === "number" ? links.get(offset) : undefined
      if (resolved && node.type === "link") {
        node.url = workspaceFileLinkUri(resolved)
      }
      if (resolved && node.type === "inlineCode") {
        node.type = "link"
        node.url = workspaceFileLinkUri(resolved)
        node.children = [{ type: "text", value: node.value }]
        delete node.value
      }
      for (const child of node.children ?? []) visit(child)
    }
    visit(tree)
  }
}
