import { getTransport } from "@/lib/transport"

export function getKeepAwakeEnabled(): Promise<boolean> {
  return getTransport().call("get_keep_awake_enabled")
}

export function setKeepAwakeEnabled(enabled: boolean): Promise<boolean> {
  return getTransport().call("set_keep_awake_enabled", { enabled })
}
