import { normalizeAbsPath } from "@/lib/file-open-target"

export const PREVIEW_IMAGE_ORIGIN = "https://workspace-preview.invalid/"

interface Node {
  type: string
  url?: string
  value?: string
  alt?: string | null
  identifier?: string
  depth?: number
  children?: Node[]
  data?: { hProperties?: Record<string, unknown> } | undefined
}

export function nodeText(node: Node): string {
  return node.value ?? node.children?.map(nodeText).join("") ?? ""
}

export function headingSlug(value: string): string {
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/[^\p{L}\p{N}_\s-]/gu, "")
      .replace(/\s/g, "-") || "section"
  )
}

export function headingId(value: string, counts: Map<string, number>): string {
  const slug = headingSlug(value)
  const count = counts.get(slug) ?? 0
  counts.set(slug, count + 1)
  return count ? `${slug}-${count}` : slug
}

export function markdownPreviewPaths(options: {
  rootPath: string
  path: string
}) {
  return (tree: Node) => {
    const headings = new Map<string, number>()
    const definitions = new Map<string, string>()
    walk(tree, (node) => {
      if (node.type === "definition" && node.identifier && node.url)
        definitions.set(node.identifier, node.url)
    })
    walk(tree, (node) => {
      if (node.type === "heading") {
        node.data = {
          ...node.data,
          hProperties: {
            ...node.data?.hProperties,
            id: headingId(nodeText(node), headings),
          },
        }
      }
      if (node.type === "imageReference" && node.identifier) {
        node.url = definitions.get(node.identifier)
        node.type = "image"
      }
      if (node.type !== "image" || !node.url) return
      if (!options.rootPath) {
        if (/^https?:/.test(options.path)) {
          try {
            node.url = new URL(node.url, options.path).href
          } catch {
            /* 保留无法解析的引用 */
          }
        }
        return
      }
      if (/^(https?:|data:|blob:|\/\/)/i.test(node.url)) return
      const source = node.url.split(/[?#]/)[0]
      let decoded: string
      try {
        decoded = decodeURIComponent(source)
      } catch {
        decoded = source
      }
      const directory = options.path.replace(/\\/g, "/").replace(/\/[^/]*$/, "")
      const target = /^[a-z]:[\\/]/i.test(decoded)
        ? decoded
        : decoded.startsWith("/")
          ? `${options.rootPath}/${decoded.slice(1)}`
          : `${directory}/${decoded}`
      node.url = `${PREVIEW_IMAGE_ORIGIN}${encodeURIComponent(normalizeAbsPath(target))}`
    })
  }
}

function walk(node: Node, visit: (node: Node) => void) {
  visit(node)
  node.children?.forEach((child) => walk(child, visit))
}
