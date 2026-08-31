import { mergeAttributes, Node, type JSONContent } from "@tiptap/core"
import { ReactNodeViewRenderer } from "@tiptap/react"

import { ScenarioVariableView } from "./scenario-variable-view"

export const SCENARIO_VARIABLE_NODE = "scenarioVariable"

export interface ScenarioVariableAttrs {
  key: string
  label: string
  type: "input" | "select"
  options: string[]
  defaultValue: string
  value: string
  required: boolean
  allowCustom: boolean
  placeholder: string
  customMode: boolean
}

export const ScenarioVariable = Node.create({
  name: SCENARIO_VARIABLE_NODE,
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return {
      key: { default: "" },
      label: { default: "" },
      type: { default: "input" },
      options: { default: [] },
      defaultValue: { default: "" },
      value: { default: "" },
      required: { default: false },
      allowCustom: { default: true },
      placeholder: { default: "" },
      customMode: { default: false },
    }
  },

  parseHTML() {
    return [{ tag: "span[data-scenario-variable]" }]
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { "data-scenario-variable": "" }),
      scenarioVariableText(node.attrs as ScenarioVariableAttrs),
    ]
  },

  renderText({ node }) {
    return scenarioVariableText(node.attrs as ScenarioVariableAttrs)
  },

  renderMarkdown(node: JSONContent) {
    return scenarioVariableText(node.attrs as ScenarioVariableAttrs)
  },

  addNodeView() {
    return ReactNodeViewRenderer(ScenarioVariableView)
  },
})

export function scenarioVariableText(
  attrs: Partial<ScenarioVariableAttrs>
): string {
  const key = String(attrs.key ?? "").trim()
  const value = String(attrs.value ?? "").trim()
  return value || `{{${key}}}`
}
