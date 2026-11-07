import { getTransport } from "@/lib/transport"
import type { AgentType } from "@/lib/types"
import type { SkillPluginPermissions } from "@/lib/skill-market"

export interface PluginPreparation {
  slug: string
  installedVersion: string
  schemaVersion: number
  state:
    | "blocked"
    | "legacy"
    | "context_required"
    | "configuration_required"
    | "permission_required"
    | "disabled"
    | "unchecked"
  issues: string[]
  dependencies: {
    verified: boolean
    issues: string[]
    dependencies: Array<{
      packageType: "skill" | "expert" | "plugin"
      id: string
      slug: string
      requiredVersion: string
      installedVersion: string | null
      ready: boolean
      reason: string | null
    }>
  }
  permissions: SkillPluginPermissions | null
  permissionGranted: boolean
  connectors: PluginConnectorPreparation[]
  contextToken: string | null
  workspaceName: string | null
  agentType: AgentType | null
}

export interface PluginParameterState {
  key: string
  label: string
  description: string
  required: boolean
  secret: boolean
  defaultValue: string | null
  placeholder: string
  value: string | null
  configured: boolean
}

export interface PluginConnectorPreparation {
  key: string
  enabled: boolean
  configurationReady: boolean
  parameters: PluginParameterState[]
  hasAuthentication: boolean
  canLogin: boolean
  canLogout: boolean
}

export interface PluginAccountStatus {
  state:
    | "not_required"
    | "authenticated"
    | "authentication_required"
    | "login_started"
    | "disconnected"
  authenticated: boolean | null
  authorizationUrl: string | null
}

export const configurePluginPreparation = (
  input: PluginPreparationChange & {
    connectorKey: string
    values: Record<string, string>
    clearKeys: string[]
  },
  transport = getTransport()
) =>
  transport.call<PluginPreparation>(
    "plugin_preparation_configure",
    { input },
    { timeoutMs: 700_000 }
  )

export const operatePluginAccount = (
  input: PluginPreparationChange & {
    connectorKey: string
    operation: "status" | "login" | "logout"
  },
  transport = getTransport()
) =>
  transport.call<PluginAccountStatus>(
    "plugin_preparation_account",
    { input },
    { timeoutMs: 700_000 }
  )

export interface PluginPreparationChange {
  marketSkillId: string
  conversationId: number
  contextToken: string
}

export interface PluginConnectionCheck {
  state: "connection_verified"
  toolCount: number
}

export const readPluginPreparation = (
  input: { marketSkillId: string; conversationId: number | null },
  transport = getTransport()
) => transport.call<PluginPreparation>("plugin_preparation_get", { input })

export const changePluginPreparation = (
  input: PluginPreparationChange,
  enabled: boolean,
  transport = getTransport()
) =>
  transport.call<PluginPreparation>(
    enabled ? "plugin_preparation_approve" : "plugin_preparation_disable",
    { input },
    { timeoutMs: 700_000 }
  )

export const checkPluginPreparation = (
  input: PluginPreparationChange,
  transport = getTransport()
) =>
  transport.call<PluginConnectionCheck>(
    "plugin_preparation_check",
    { input },
    { timeoutMs: 700_000 }
  )
