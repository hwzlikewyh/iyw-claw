"use client"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { McpMarketplaceInstallOption } from "@/lib/types"

type Field = McpMarketplaceInstallOption["parameters"][number]

interface FieldsProps {
  fields: Field[]
  values: Record<string, string>
  disabled: boolean
  onChange: (key: string, value: string) => void
}

export class ConnectorParameterError extends Error {
  constructor(
    field: string,
    readonly reason: "required" | "invalid"
  ) {
    super(field)
  }
}

export function ConnectorInstallFields(props: FieldsProps) {
  return props.fields.map((field) => (
    <div key={field.key} className="grid gap-1.5">
      <Label htmlFor={`connector-param-${field.key}`}>
        {field.label}
        {field.required ? " *" : ""}
      </Label>
      <ParameterInput field={field} form={props} />
      {field.description ? (
        <p className="text-xs text-muted-foreground">{field.description}</p>
      ) : null}
    </div>
  ))
}

function ParameterInput({ field, form }: { field: Field; form: FieldsProps }) {
  const id = `connector-param-${field.key}`
  const value = form.values[field.key] ?? ""
  const choices = field.enum_values?.length
    ? field.enum_values
    : field.kind === "boolean"
      ? ["true", "false"]
      : null
  const placeholder =
    field.placeholder ??
    (field.default_value == null ? "" : String(field.default_value))
  if (choices && !field.secret) {
    return (
      <select
        id={id}
        className="h-10 rounded-md border bg-background px-2 text-sm"
        value={value}
        disabled={form.disabled}
        onChange={(event) => form.onChange(field.key, event.target.value)}
      >
        <option value="">{placeholder || "—"}</option>
        {choices.map((choice) => (
          <option key={choice} value={choice}>
            {choice}
          </option>
        ))}
      </select>
    )
  }
  return (
    <Input
      id={id}
      type={field.secret ? "password" : "text"}
      autoComplete="off"
      placeholder={placeholder}
      value={value}
      disabled={form.disabled}
      onChange={(event) => form.onChange(field.key, event.target.value)}
    />
  )
}

export function connectorParameterValues(
  fields: Field[],
  values: Record<string, string>
): Record<string, unknown> {
  return Object.fromEntries(
    fields.flatMap((field) => {
      const raw = values[field.key]
      if (raw == null || raw === "") {
        if (field.required && field.default_value == null)
          throw new ConnectorParameterError(field.label, "required")
        return []
      }
      try {
        const value = parseParameter(raw, field.kind)
        if (
          field.enum_values?.length &&
          !field.enum_values.includes(String(value))
        )
          throw new Error("enum")
        return [[field.key, value]]
      } catch {
        throw new ConnectorParameterError(field.label, "invalid")
      }
    })
  )
}

function parseParameter(raw: string, kind: string): unknown {
  if (kind === "json") return JSON.parse(raw)
  if (kind === "boolean") {
    if (!["true", "false"].includes(raw)) throw new Error("boolean")
    return raw === "true"
  }
  if (kind === "number" || kind === "integer") {
    if (!raw.trim()) throw new Error("number")
    const value = Number(raw)
    if (
      !Number.isFinite(value) ||
      (kind === "integer" && !Number.isSafeInteger(value))
    )
      throw new Error("number")
    return value
  }
  return raw
}
