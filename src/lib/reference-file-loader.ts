import ig from "ignore"
import { getFileTree, readFilePreview } from "@/lib/api"
import { joinFsPath } from "@/lib/path-utils"
import type { FlatFileEntry } from "@/hooks/use-file-tree"
import type { FileTreeNode } from "@/lib/types"

const SEARCH_DEPTH = 10
const DIRECTORY_CONCURRENCY = 4

interface IgnoreRule {
  prefix: string
  matcher: ReturnType<typeof ig>
}

interface Directory {
  path: string
  depth: number
  rules: IgnoreRule[]
}

function visibleEntry(node: FileTreeNode, directory: Directory) {
  if (node.name === ".gitignore") return null
  const relativePath = directory.path
    ? `${directory.path}/${node.path}`
    : node.path
  for (const { prefix, matcher } of directory.rules) {
    const relative = relativePath.slice(prefix.length)
    if (matcher.ignores(node.kind === "dir" ? `${relative}/` : relative)) {
      return null
    }
  }
  return {
    name: node.name,
    relativePath,
    kind: node.kind,
    lowerPath: relativePath.toLowerCase(),
    lowerName: node.name.toLowerCase(),
  }
}

async function readDirectory(
  root: string,
  directory: Directory,
  signal: AbortSignal
): Promise<{ entries: FlatFileEntry[]; children: Directory[] }> {
  signal.throwIfAborted()
  const nodes = await getFileTree(joinFsPath(root, directory.path), 1)
  signal.throwIfAborted()
  if (
    nodes.some((node) => node.name === ".gitignore" && node.kind === "file")
  ) {
    const prefix = directory.path ? `${directory.path}/` : ""
    try {
      const result = await readFilePreview(root, `${prefix}.gitignore`)
      directory = {
        ...directory,
        rules: [
          ...directory.rules,
          { prefix, matcher: ig().add(result.content) },
        ],
      }
    } catch {
      // 与原文件搜索一致，不可读的忽略文件不阻止其余目录加载。
    }
    signal.throwIfAborted()
  }
  const entries: FlatFileEntry[] = []
  const children: Directory[] = []
  for (const node of nodes) {
    const entry = visibleEntry(node, directory)
    if (!entry) continue
    entries.push(entry)
    if (node.kind === "dir" && directory.depth + 1 < SEARCH_DEPTH) {
      children.push({
        path: entry.relativePath,
        depth: directory.depth + 1,
        rules: directory.rules,
      })
    }
  }
  return { entries, children }
}

/** 逐层过滤后才进入目录，避免把忽略的依赖/构建树经 IPC 整体复制到前端。 */
export async function loadReferenceFiles(root: string, signal: AbortSignal) {
  const pending: Directory[] = [{ path: "", depth: 0, rules: [] }]
  const files: FlatFileEntry[] = []
  while (pending.length) {
    signal.throwIfAborted()
    const batch = pending.splice(0, DIRECTORY_CONCURRENCY)
    const results = await Promise.all(
      batch.map((directory) => readDirectory(root, directory, signal))
    )
    signal.throwIfAborted()
    for (const result of results) {
      for (const entry of result.entries) files.push(entry)
      for (const child of result.children) pending.push(child)
    }
  }
  return files
}
