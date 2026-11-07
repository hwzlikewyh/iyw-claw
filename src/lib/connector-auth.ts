import { getTransport } from "@/lib/transport"

export interface ConnectorCheck {
  state: "ready" | "authentication_required" | "failed" | "unverified"
  message: string
  toolCount: number | null
}

export const checkConnector = (serverId: string, transport = getTransport()) =>
  transport.call<ConnectorCheck>(
    "mcp_connector_check",
    { serverId },
    { timeoutMs: 80_000 }
  )

export const beginConnectorCLI = (
  serverId: string,
  terminalId: string,
  transport = getTransport()
) => transport.call<void>("mcp_cli_begin", { serverId, terminalId })

export const cancelConnectorCLI = (
  terminalId: string,
  transport = getTransport()
) => transport.call<void>("mcp_cli_cancel", { terminalId })

export const logoutConnectorCLI = (serverId: string) =>
  getTransport().call<void>("mcp_cli_logout", { serverId })
