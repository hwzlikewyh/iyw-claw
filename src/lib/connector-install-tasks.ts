import { getTransport } from "@/lib/transport"

export interface ConnectorInstallTask {
  id: string
  serverId: string
  optionId: string | null
  phase:
    | "preparing"
    | "checking_cache"
    | "downloading"
    | "verifying"
    | "extracting"
    | "committing"
    | "cancelling"
    | "completed"
    | "cancelled"
    | "failed"
    | "interrupted"
  receivedBytes: number
  totalBytes: number | null
  cancellable: boolean
  error: string | null
  createdAt: string
}

export interface ConnectorInstallInput {
  providerId: string
  serverId: string
  optionId: string | null
  protocol: string | null
  parameterValues: Record<string, unknown>
}

export function connectorTaskActive(task: ConnectorInstallTask) {
  return !["completed", "failed", "cancelled", "interrupted"].includes(
    task.phase
  )
}

export function listConnectorInstallTasks(): Promise<ConnectorInstallTask[]> {
  return getTransport().call("mcp_market_install_list")
}

export function beginConnectorInstall(input: ConnectorInstallInput) {
  return getTransport().call<ConnectorInstallTask>("mcp_market_install_begin", {
    input,
  })
}

export function cancelConnectorInstall(taskId: string) {
  return getTransport().call<ConnectorInstallTask>(
    "mcp_market_install_cancel",
    {
      taskId,
    }
  )
}
