import { getTransport } from "@/lib/transport"

export interface UiSettings {
  enabled: boolean
}
export interface UiState {
  conversationId: number
  turnId: string
  blockId: string
  sourceHash: string
  stateJson: string
  revision: number
}
export interface UiStateInput {
  conversationId: number
  blockId: string
  source: string
  state: Record<string, unknown>
  revision: number
}
export const UI_SETTINGS_EVENT = "intelligent-ui://settings"
export const UI_STATE_EVENT = "intelligent-ui://state"
export const getUiSettings = () =>
  getTransport().call<UiSettings>("get_intelligent_ui_settings")
export const setUiSettings = (settings: UiSettings) =>
  getTransport().call<UiSettings>("set_intelligent_ui_settings", { settings })
export const readUiStates = (conversationId: number) =>
  getTransport().call<UiState[]>("read_intelligent_ui_states", {
    conversationId,
  })
export const saveUiState = (input: UiStateInput) =>
  getTransport().call<UiState>("save_intelligent_ui_state", { input })
