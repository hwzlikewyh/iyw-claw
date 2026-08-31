import type { ChangeEvent, SyntheticEvent } from "react"
import { NodeViewWrapper, type ReactNodeViewProps } from "@tiptap/react"

import type { ScenarioVariableAttrs } from "./scenario-variable-node"

const CUSTOM_VALUE = "__scenario_custom__"

interface ScenarioSelectProps {
  attrs: ScenarioVariableAttrs
  options: string[]
  value: string
  updateAttributes: ReactNodeViewProps["updateAttributes"]
  stop: (event: SyntheticEvent) => void
}

function ScenarioSelect({
  attrs,
  options,
  value,
  updateAttributes,
  stop,
}: ScenarioSelectProps) {
  const hasCustomOption = options.includes("自定义")
  const handleChange = (event: ChangeEvent<HTMLSelectElement>) => {
    const next = event.target.value
    updateAttributes(
      next === CUSTOM_VALUE
        ? { customMode: true, value: "" }
        : { customMode: false, value: next }
    )
  }

  return (
    <select
      aria-label={attrs.label}
      value={value}
      onChange={handleChange}
      onBlur={stop}
    >
      <option value="">请选择</option>
      {options.map((option) => {
        if (option === "自定义" && !attrs.allowCustom) return null
        const optionValue = option === "自定义" ? CUSTOM_VALUE : option
        return (
          <option key={optionValue} value={optionValue}>
            {option}
          </option>
        )
      })}
      {attrs.allowCustom && !hasCustomOption ? (
        <option value={CUSTOM_VALUE}>自定义</option>
      ) : null}
    </select>
  )
}

export function ScenarioVariableView({
  node,
  updateAttributes,
}: ReactNodeViewProps) {
  const attrs = node.attrs as ScenarioVariableAttrs
  const options = Array.isArray(attrs.options) ? attrs.options : []
  const configuredCustom = attrs.value === "自定义" && attrs.allowCustom
  const custom = attrs.type === "input" || attrs.customMode || configuredCustom
  const selectValue = custom ? CUSTOM_VALUE : attrs.value
  const inputValue = configuredCustom ? "" : attrs.value

  const stop = (event: SyntheticEvent) => event.stopPropagation()

  return (
    <NodeViewWrapper
      as="span"
      className="iyw-claw-scenario-variable"
      contentEditable={false}
      data-scenario-variable-key={attrs.key}
      data-scenario-type={attrs.type}
      data-scenario-custom={custom ? "true" : undefined}
      data-scenario-filled={String(attrs.value ?? "").trim() ? "true" : "false"}
      onMouseDown={stop}
      onClick={stop}
    >
      <span className="iyw-claw-scenario-variable-label">{attrs.label}</span>
      <span className="iyw-claw-scenario-variable-value">
        {attrs.type === "select" ? (
          <ScenarioSelect
            attrs={attrs}
            options={options}
            value={selectValue}
            updateAttributes={updateAttributes}
            stop={stop}
          />
        ) : null}
        {custom ? (
          <input
            aria-label={`${attrs.label}自定义值`}
            value={inputValue}
            placeholder={attrs.placeholder || "填写"}
            onChange={(event) =>
              updateAttributes({ value: event.target.value })
            }
            onKeyDown={stop}
            onBlur={stop}
          />
        ) : null}
      </span>
    </NodeViewWrapper>
  )
}
