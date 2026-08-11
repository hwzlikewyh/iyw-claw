import {
  normalizeAppUpdateError,
  type AppUpdateErrorKind,
  type AppUpdateInfo,
  type AppUpdateState,
} from "@/lib/updater"

export const ACTIVE_UPDATE_STATUSES = new Set<AppUpdateState["status"]>([
  "checking",
  "downloading",
  "verifying",
  "installing",
  "restarting",
])

export interface UpdateDetails {
  currentVersion: string
  canInstall: boolean
  checked: boolean
  checkedSeq: number | null
  liveProgress: boolean | null
  loaded: boolean
  runtime?: string
  availableUpdate: AppUpdateInfo | null
}

export interface UpdateDialogState {
  details: UpdateDetails
  checking: boolean
  checkErrorKind: AppUpdateErrorKind | null
  runCheck: () => Promise<void>
}

export type UpdateErrorKey =
  | "updateErrors.sourceUnavailable"
  | "updateErrors.network"
  | "updateErrors.downloadFailed"
  | "updateErrors.installFailed"
  | "updateErrors.unknown"

export function hasFreshCheck(
  state: AppUpdateState,
  details: UpdateDetails
): boolean {
  return details.checked && details.checkedSeq === state.seq
}

export function getLifecycleErrorKind(
  state: AppUpdateState,
  details: UpdateDetails
): AppUpdateErrorKind | null {
  if (
    state.status !== "error" ||
    !state.error ||
    hasFreshCheck(state, details)
  ) {
    return null
  }
  return normalizeAppUpdateError(state.error).kind
}

export function getVisibleUpdate(
  state: AppUpdateState,
  details: UpdateDetails
): AppUpdateInfo | null {
  if (state.status === "error") {
    return hasFreshCheck(state, details) ? details.availableUpdate : null
  }
  if (!state.version) {
    return hasFreshCheck(state, details) ? details.availableUpdate : null
  }
  return {
    version: state.version,
    body: state.notes ?? "",
    date: state.pubDate,
    releaseId: state.releaseId,
    channel: state.channel,
    updatePolicy: state.updatePolicy,
    enforceAfter: state.enforceAfter,
  }
}

export function getUpdateErrorKey(
  kind: AppUpdateErrorKind,
  action: "check" | "install"
): UpdateErrorKey {
  switch (kind) {
    case "source_unreachable":
      return "updateErrors.sourceUnavailable"
    case "network":
      return "updateErrors.network"
    case "download_failed":
      return "updateErrors.downloadFailed"
    case "install_failed":
      return "updateErrors.installFailed"
    case "unknown":
    default:
      return action === "install"
        ? "updateErrors.installFailed"
        : "updateErrors.unknown"
  }
}
