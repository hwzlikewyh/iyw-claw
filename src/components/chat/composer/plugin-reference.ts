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
  editor.state.doc.descendants((node) => {
    if (node.type.name === "reference" && node.attrs.refType === "skill") {
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
      if (hasExistingPluginReference || index > 0) {
        reference.meta = { ...reference.meta, hiddenFromDisplay: true }
      }
      return reference
    })
  if (!references.length && input.plugin.skills.length) return
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
  if (!input.plugin.skills.length) {
    chain = chain.insertContent({ type: "text", text: input.instruction })
  }
  chain.run()
}
