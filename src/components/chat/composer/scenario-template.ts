import type { JSONContent } from "@tiptap/core"

import type { ScenarioVariable } from "@/lib/types"

import {
  SCENARIO_VARIABLE_NODE,
  type ScenarioVariableAttrs,
} from "./nodes/scenario-variable-node"

const PLACEHOLDER = /{{\s*([a-z][a-z0-9_]*)\s*}}/gi

const FALLBACK_LABELS: Record<string, string> = {
  market: "目标市场",
  category: "品类",
  period: "时间范围",
  format: "输出格式",
  platform: "平台",
  files: "文件",
  materials: "已有资料",
}

function variableAttrs(variable: ScenarioVariable): ScenarioVariableAttrs {
  const defaultValue = variable.defaultValue?.trim() ?? ""
  const options = variable.options ?? []
  return {
    key: variable.key,
    label: variable.label,
    type: variable.type,
    options,
    defaultValue,
    value: defaultValue,
    required: variable.required ?? false,
    allowCustom: variable.allowCustom ?? true,
    placeholder: variable.placeholder ?? "",
    customMode:
      variable.type === "input" ||
      (defaultValue !== "" && !options.includes(defaultValue)),
  }
}

function lineContent(
  line: string,
  variables: Map<string, ScenarioVariable>
): JSONContent[] {
  const content: JSONContent[] = []
  let cursor = 0
  for (const match of line.matchAll(PLACEHOLDER)) {
    const key = match[1]?.toLowerCase()
    if (!key) continue
    const start = match.index ?? 0
    if (start > cursor)
      content.push({ type: "text", text: line.slice(cursor, start) })
    const variable = variables.get(key)
    if (variable) {
      content.push({
        type: SCENARIO_VARIABLE_NODE,
        attrs: variableAttrs(variable),
      })
    } else {
      content.push({ type: "text", text: match[0] })
    }
    cursor = start + match[0].length
  }
  if (cursor < line.length)
    content.push({ type: "text", text: line.slice(cursor) })
  return content
}

export function scenarioTemplateToDoc(
  template: string,
  variables: ScenarioVariable[]
): JSONContent {
  const lookup = new Map(
    variables.map((variable) => [variable.key.toLowerCase(), variable])
  )
  const lines = template.split("\n")
  const content: JSONContent[] = []
  lines.forEach((line, index) => {
    if (index > 0) content.push({ type: "hardBreak" })
    content.push(...lineContent(line, lookup))
  })
  return { type: "doc", content: [{ type: "paragraph", content }] }
}

/** Preserve a usable editor when an older catalog has no variable metadata. */
export function inferScenarioVariables(template: string): ScenarioVariable[] {
  const keys = [
    ...new Set(
      [...template.matchAll(PLACEHOLDER)].map((match) =>
        String(match[1]).toLowerCase()
      )
    ),
  ]
  return keys.map((key) => ({
    key,
    label: FALLBACK_LABELS[key] ?? key,
    type: "input",
    options: [],
    required: false,
    allowCustom: true,
    placeholder: `填写${FALLBACK_LABELS[key] ?? key}`,
  }))
}

export function missingScenarioVariables(doc: JSONContent): string[] {
  const missing: string[] = []
  const visit = (node: JSONContent) => {
    if (node.type === SCENARIO_VARIABLE_NODE) {
      const attrs = (node.attrs ?? {}) as Partial<ScenarioVariableAttrs>
      if (attrs.required && !String(attrs.value ?? "").trim()) {
        missing.push(String(attrs.label ?? attrs.key ?? "字段"))
      }
    }
    node.content?.forEach(visit)
  }
  visit(doc)
  return [...new Set(missing)]
}
