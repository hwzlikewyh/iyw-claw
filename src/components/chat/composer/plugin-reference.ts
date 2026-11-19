import type { Editor } from "@tiptap/core"
import type { ComposerPluginChoice } from "@/lib/composer-plugins"
import { skillToReference, type InvocationPrefix } from "./invocation-reference"

export function insertPluginReferences(
  editor: Editor,
  input: {
    plugin: ComposerPluginChoice
    prefix: InvocationPrefix
    instruction: string
  }
) {
  const existing = new Set<string>()
  let hasPluginSource = false
  editor.state.doc.descendants((node) => {
    if (node.type.name === "reference" && node.attrs.refType === "skill") {
      if (
        node.attrs.meta?.composerSource?.kind === "plugin" &&
        node.attrs.meta.composerSource.slug === input.plugin.slug
      )
        hasPluginSource = true
      existing.add(String(node.attrs.id))
    }
  })
  const hasExistingPluginReference = input.plugin.skills.some((skill) =>
    existing.has(skill.id)
  )
  const references = input.plugin.skills
    .filter((skill) => !existing.has(skill.id))
    .map((skill, index) => {
      const reference = skillToReference(
        { ...skill, name: input.plugin.label },
        input.prefix
      )
      reference.meta = {
        ...reference.meta,
        composerSource: { kind: "plugin", slug: input.plugin.slug },
      }
      if (hasExistingPluginReference || index > 0) {
        reference.meta = { ...reference.meta, hiddenFromDisplay: true }
      }
      return reference
    })
  if (!references.length && hasPluginSource) return
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
  let chain = editor.chain().focus()
  if (charBefore && !/\s/.test(charBefore)) chain = chain.insertContent(" ")
  for (const reference of references) {
    chain = chain.insertReference(reference).insertContent(" ")
  }
  if (!references.length) {
    chain = chain
      .insertReference({
        refType: "skill",
        id: input.plugin.slug,
        label: input.plugin.label,
        uri: null,
        meta: {
          composerSource: {
            kind: "plugin",
            slug: input.plugin.slug,
            instruction: input.instruction,
          },
        },
      })
      .insertContent(" ")
  }
  chain.run()
}
