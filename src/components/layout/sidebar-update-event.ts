export const OPEN_APP_UPDATE_EVENT = "iyw:open-app-update"

export function requestAppUpdateDialog() {
  window.dispatchEvent(new Event(OPEN_APP_UPDATE_EVENT))
}
