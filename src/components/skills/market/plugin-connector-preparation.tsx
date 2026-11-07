"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type {
  PluginPreparation,
  PluginConnectorPreparation,
} from "@/lib/plugin-preparation"
import type { PluginPreparationController } from "./use-plugin-preparation"
import { PluginConfigurationDialog } from "./plugin-configuration-dialog"
import { PluginAccountPanel } from "./plugin-account-panel"

export function PluginConnectorPreparationList({
  id,
  conversationId,
  state,
}: {
  id: string
  conversationId: number | null
  state: PluginPreparationController
}) {
  const view = state.view
  if (!view || view.schemaVersion < 2) return null
  return (
    <div className="space-y-4">
      {view.connectors
        .filter(
          (connector) =>
            connector.parameters?.length || connector.hasAuthentication
        )
        .map((connector) => (
          <ConnectorPreparation
            key={`${view.contextToken}:${connector.key}`}
            id={id}
            conversationId={conversationId}
            view={view}
            connector={connector}
            state={state}
          />
        ))}
    </div>
  )
}

interface Props {
  id: string
  conversationId: number | null
  view: PluginPreparation
  connector: PluginConnectorPreparation
  state: PluginPreparationController
}

function ConnectorPreparation(props: Props) {
  const [configuring, setConfiguring] = useState(false)
  const { view, connector, state, conversationId } = props
  const disabled =
    state.busy || !!state.error || conversationId == null || !view.contextToken
  const input = {
    marketSkillId: props.id,
    conversationId: conversationId || 0,
    contextToken: view.contextToken || "",
  }
  return (
    <div className="space-y-2 rounded-md bg-muted/40 p-3">
      <h4 className="break-words text-sm font-medium">{connector.key}</h4>
      <ConfigurationAction
        connector={connector}
        disabled={disabled}
        onOpen={() => setConfiguring(true)}
      />
      {connector.hasAuthentication ? (
        <PluginAccountPanel
          input={input}
          connector={connector}
          disabled={
            disabled ||
            !view.permissionGranted ||
            !connector.enabled ||
            !connector.configurationReady
          }
          onChanged={state.clearConnection}
        />
      ) : null}
      {configuring && conversationId != null ? (
        <PluginConfigurationDialog
          id={props.id}
          conversationId={conversationId}
          view={view}
          connector={connector}
          onClose={() => setConfiguring(false)}
          onSaved={state.applyView}
        />
      ) : null}
    </div>
  )
}

function ConfigurationAction({
  connector,
  disabled,
  onOpen,
}: {
  connector: PluginConnectorPreparation
  disabled: boolean
  onOpen: () => void
}) {
  const t = useTranslations("PluginPreparation")
  if (!connector.parameters.length) return null
  return (
    <>
      <p className="text-[13px]">
        {t(
          connector.configurationReady
            ? "configurationSaved"
            : "configurationRequired"
        )}
      </p>
      <Button size="sm" variant="outline" disabled={disabled} onClick={onOpen}>
        {t("configureParameters")}
      </Button>
    </>
  )
}
