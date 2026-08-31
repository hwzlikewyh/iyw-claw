import type { SyntheticEvent } from "react"
import { NodeViewWrapper, type ReactNodeViewProps } from "@tiptap/react"

import type { ScenarioVariableAttrs } from "./scenario-variable-node"

const CUSTOM_VALUE = "__scenario_custom__"

export function ScenarioVariableView({
  node,
  updateAttributes,
}: ReactNodeViewProps) {
  const attrs = node.attrs as ScenarioVariableAttrs
  const options = Array.isArray(attrs.options) ? attrs.options : []
  const hasCustomOption = options.some((option) => option === "自定义")
  const configuredCustom = attrs.value === "自定义" && attrs.allowCustom
  const custom = attrs.type === "input" || attrs.customMode || configuredCustom
  const selectValue = custom ? CUSTOM_VALUE : attrs.value
  const inputValue = configuredCustom ? "" : attrs.value

  const stop = (event: SyntheticEvent) => event.stopPropagation()
  const setValue = (value: string) => updateAttributes({ value })

  return (
    <NodeViewWrapper
      as="span"
      className="iyw-claw-scenario-variable"
      contentEditable={false}
      data-scenario-variable-key={attrs.key}
      onMouseDown={stop}
      onClick={stop}
    >
      <span className="iyw-claw-scenario-variable-label">{attrs.label}</span>
      {attrs.type === "select" ? (
        <select
          aria-label={attrs.label}
          value={selectValue}
          onChange={(event) => {
            const value = event.target.value
            if (value === CUSTOM_VALUE) {
              updateAttributes({ customMode: true, value: "" })
            } else {
              updateAttributes({ customMode: false, value })
            }
          }}
          onBlur={stop}
        >
          <option value="">请选择</option>
          {options.map((option) =>
            option === "自定义" && attrs.allowCustom ? (
              <option key={CUSTOM_VALUE} value={CUSTOM_VALUE}>
                自定义
              </option>
            ) : option === "自定义" ? null : (
              <option key={option} value={option}>
                {option}
              </option>
            )
          )}
          {attrs.allowCustom && !hasCustomOption ? (
            <option value={CUSTOM_VALUE}>自定义</option>
          ) : null}
        </select>
      ) : null}
      {custom ? (
        <input
          aria-label={`${attrs.label}自定义值`}
          value={inputValue}
          placeholder={attrs.placeholder || "填写"}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={stop}
          onBlur={stop}
        />
      ) : null}
    </NodeViewWrapper>
  )
}
