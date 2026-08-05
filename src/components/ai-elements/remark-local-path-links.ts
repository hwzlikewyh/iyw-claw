import {
  createLocalPathUri,
  findLocalPathMatches,
  type LocalPathPresentation,
} from "@/lib/local-path-links"

type MdastNodeLike = {
  type: string
  value?: unknown
  url?: unknown
  children?: MdastNodeLike[]
}

function localPathLink(path: string, presentation: LocalPathPresentation) {
  return {
    type: "link",
    url: createLocalPathUri(path, presentation),
    children: [{ type: "text", value: path }],
  }
}

function splitTextNode(
  value: string,
  presentation: LocalPathPresentation,
  plainType: "text" | "inlineCode",
  wholeLine = false
): MdastNodeLike[] | null {
  const matches = findLocalPathMatches(value, { wholeLine })
  if (matches.length === 0) return null

  const nodes: MdastNodeLike[] = []
  let cursor = 0
  for (const match of matches) {
    if (match.start > cursor) {
      nodes.push({ type: plainType, value: value.slice(cursor, match.start) })
    }
    nodes.push(localPathLink(match.path, presentation))
    cursor = match.end
  }
  if (cursor < value.length) {
    nodes.push({ type: plainType, value: value.slice(cursor) })
  }
  return nodes
}

function transformChildren(parent: MdastNodeLike): void {
  if (!Array.isArray(parent.children)) return

  const nextChildren: MdastNodeLike[] = []
  for (const child of parent.children) {
    if (child.type === "link" || child.type === "image") {
      nextChildren.push(child)
      continue
    }

    if (child.type === "text" && typeof child.value === "string") {
      nextChildren.push(
        ...(splitTextNode(child.value, "text", "text") ?? [child])
      )
      continue
    }

    if (child.type === "inlineCode" && typeof child.value === "string") {
      nextChildren.push(
        ...(splitTextNode(child.value, "inline-code", "inlineCode", true) ?? [
          child,
        ])
      )
      continue
    }

    transformChildren(child)
    nextChildren.push(child)
  }
  parent.children = nextChildren
}

export function remarkLocalPathLinks() {
  return (tree: MdastNodeLike) => transformChildren(tree)
}
