import { Extension, type Editor } from "@tiptap/core"
import { Plugin, PluginKey, type EditorState } from "@tiptap/pm/state"
import { Decoration, DecorationSet } from "@tiptap/pm/view"

const realtimePartialKey = new PluginKey<string>("realtimeVoicePartial")

function partialDecorations(state: EditorState): DecorationSet | null {
  const text = realtimePartialKey.getState(state)?.trim()
  if (!text) return null
  const position = Math.max(1, state.doc.content.size - 1)
  return DecorationSet.create(state.doc, [
    Decoration.widget(
      position,
      () => {
        const span = document.createElement("span")
        span.className = "iyw-claw-realtime-voice-partial"
        span.textContent = text
        span.setAttribute("aria-hidden", "true")
        return span
      },
      { side: 1, key: `realtime-voice:${text}` }
    ),
  ])
}

export const RealtimeVoicePartial = Extension.create({
  name: "realtimeVoicePartial",

  addProseMirrorPlugins() {
    return [
      new Plugin<string>({
        key: realtimePartialKey,
        state: {
          init: () => "",
          apply: (transaction, current) => {
            const next = transaction.getMeta(realtimePartialKey)
            return typeof next === "string" ? next : current
          },
        },
        props: {
          decorations: partialDecorations,
        },
      }),
    ]
  },
})

export function setRealtimeVoicePartial(editor: Editor, text: string): void {
  editor.view.dispatch(editor.state.tr.setMeta(realtimePartialKey, text))
}
