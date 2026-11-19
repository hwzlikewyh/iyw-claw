import type { Editor } from "@tiptap/core"
import type { Node } from "@tiptap/pm/model"
import type { ReferenceAttrs } from "./types"

export type ComposerSource = {
  kind: "plugin" | "skill"
  slug: string
  label: string
  marketId?: string
}

function sourceOf(node: Node) {
  if (node.type.name !== "reference") return null
  const attrs = node.attrs as ReferenceAttrs
  const source = attrs.meta?.composerSource
  if (attrs.refType !== "skill" || !source?.slug) return null
  return {
    kind: source.kind,
    slug: source.slug,
    label: attrs.label || attrs.id,
    marketId: source.marketId,
  }
}

export function composerSources(editor: Editor | null) {
  const sources = new Map<string, ComposerSource>()
  editor?.state.doc.descendants((node) => {
    const source = sourceOf(node)
    if (source) sources.set(source.kind + ":" + source.slug, source)
  })
  return [...sources.values()]
}

export function removeComposerSource(
  editor: Editor | null,
  source: ComposerSource
) {
  if (!editor?.isEditable) return
  const tr = editor.state.tr
  editor.state.doc.descendants((node, pos) => {
    const item = sourceOf(node)
    if (item?.kind === source.kind && item.slug === source.slug) {
      const end = pos + node.nodeSize
      // 菜单插入引用后会补一个空格，一并移除以保持正文位置。
      const next = editor.state.doc.resolve(end).nodeAfter
      const separator = next?.isText && next.text?.startsWith(" ") ? 1 : 0
      tr.delete(tr.mapping.map(pos), tr.mapping.map(end + separator))
    }
  })
  if (tr.docChanged) editor.view.dispatch(tr)
  editor.commands.focus()
}
