"use client"

import { useRef, useState, useEffect } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { getTransport } from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"
import {
  configurePluginPreparation,
  type PluginPreparation,
  type PluginConnectorPreparation,
  type PluginParameterState,
} from "@/lib/plugin-preparation"

interface Props {
  id: string
  conversationId: number
  view: PluginPreparation
  connector: PluginConnectorPreparation
  onClose: () => void
  onSaved: (view: PluginPreparation) => void
}
type FormState = ReturnType<typeof useConfiguration>

export function PluginConfigurationDialog(props: Props) {
  const t = useTranslations("PluginPreparation"),
    state = useConfiguration(props)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !state.busy) props.onClose()
      }}
    >
      <DialogContent className="flex max-h-[85dvh] flex-col">
        <DialogHeader>
          <DialogTitle>{t("configurationTitle")}</DialogTitle>
          <DialogDescription>{t("configurationHint")}</DialogDescription>
        </DialogHeader>
        <div className="min-h-0 space-y-4 overflow-y-auto">
          <p className="break-words text-sm">
            {props.view.slug} · {props.view.installedVersion} ·{" "}
            {props.connector.key}
          </p>
          {props.connector.parameters.map((field) => (
            <ParameterInput key={field.key} field={field} state={state} />
          ))}
          {state.error ? (
            <p role="alert" className="break-words text-sm text-destructive">
              {state.error}
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button
            variant="outline"
            disabled={state.busy}
            onClick={props.onClose}
          >
            {t("cancel")}
          </Button>
          <Button disabled={state.busy} onClick={() => void state.save()}>
            {t(state.busy ? "saving" : "saveConfiguration")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ParameterInput({
  field,
  state,
}: {
  field: PluginParameterState
  state: FormState
}) {
  const t = useTranslations("PluginPreparation"),
    id = `plugin-param-${field.key}`
  return (
    <div className="grid gap-2 text-sm">
      <label htmlFor={id}>
        {field.label}
        {field.required ? " *" : ""}
      </label>
      <Input
        id={id}
        aria-required={field.required}
        autoComplete="off"
        type={field.secret ? "password" : "text"}
        value={state.values[field.key] || ""}
        disabled={state.busy || state.clearKeys.includes(field.key)}
        placeholder={
          field.secret && field.configured ? t("secretKeep") : field.placeholder
        }
        onChange={(event) =>
          state.setValues((current) => ({
            ...current,
            [field.key]: event.target.value,
          }))
        }
      />
      {field.description ? (
        <span className="text-[13px] text-muted-foreground">
          {field.description}
        </span>
      ) : null}
      <ClearSecret field={field} state={state} />
    </div>
  )
}

function ClearSecret({
  field,
  state,
}: {
  field: PluginParameterState
  state: FormState
}) {
  const t = useTranslations("PluginPreparation")
  return (
    <>
      {field.secret && field.configured ? (
        <label className="flex items-center gap-2 text-[13px]">
          <input
            type="checkbox"
            checked={state.clearKeys.includes(field.key)}
            disabled={state.busy}
            onChange={(event) =>
              state.setClearKeys((current) =>
                event.target.checked
                  ? [...current, field.key]
                  : current.filter((key) => key !== field.key)
              )
            }
          />
          {t("clearSecret")}
        </label>
      ) : null}
    </>
  )
}

function initialValues(connector: PluginConnectorPreparation) {
  return Object.fromEntries(
    connector.parameters
      .filter((field) => !field.secret)
      .map((field) => [field.key, field.value || ""])
  )
}

function configurationValues(
  props: Props,
  values: Record<string, string>,
  clearKeys: string[]
) {
  return Object.fromEntries(
    Object.entries(values).filter(
      ([key, value]) =>
        !clearKeys.includes(key) &&
        (!props.connector.parameters.find((field) => field.key === key)
          ?.secret ||
          value !== "")
    )
  )
}

function useConfiguration(props: Props) {
  const [transport] = useState(getTransport)
  const [values, setValues] = useState<Record<string, string>>(() =>
    initialValues(props.connector)
  )
  const [clearKeys, setClearKeys] = useState<string[]>([])
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("")
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  const save = async () => {
    if (busy || !props.view.contextToken) return
    setBusy(true)
    setError("")
    try {
      const next = await configurePluginPreparation(
        {
          marketSkillId: props.id,
          conversationId: props.conversationId,
          contextToken: props.view.contextToken,
          connectorKey: props.connector.key,
          values: configurationValues(props, values, clearKeys),
          clearKeys,
        },
        transport
      )
      if (alive.current) {
        props.onSaved(next)
        props.onClose()
      }
    } catch (cause) {
      if (alive.current)
        setError(extractAppCommandError(cause)?.message || String(cause))
    } finally {
      if (alive.current) setBusy(false)
    }
  }
  return { values, setValues, clearKeys, setClearKeys, busy, error, save }
}
