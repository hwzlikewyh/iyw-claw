const TASK_COMPLETION_NOTIFICATION_STORAGE_KEY =
  "iyw-claw-task-completion-notification"
const TASK_COMPLETION_NOTIFICATION_CHANGED_EVENT =
  "iyw-claw:task-completion-notification-changed"
const DEFAULT_TASK_COMPLETION_NOTIFICATION_ENABLED = true

export function getTaskCompletionNotificationEnabled(): boolean {
  if (typeof window === "undefined") {
    return DEFAULT_TASK_COMPLETION_NOTIFICATION_ENABLED
  }

  try {
    const value = window.localStorage.getItem(
      TASK_COMPLETION_NOTIFICATION_STORAGE_KEY
    )
    return value === null
      ? DEFAULT_TASK_COMPLETION_NOTIFICATION_ENABLED
      : value === "true"
  } catch {
    return DEFAULT_TASK_COMPLETION_NOTIFICATION_ENABLED
  }
}

export function setTaskCompletionNotificationEnabled(enabled: boolean): void {
  if (typeof window === "undefined") return

  try {
    window.localStorage.setItem(
      TASK_COMPLETION_NOTIFICATION_STORAGE_KEY,
      String(enabled)
    )
  } catch {
    // 隐私模式或嵌入式 WebView 禁用 storage 时仍更新当前窗口状态。
  }

  window.dispatchEvent(new Event(TASK_COMPLETION_NOTIFICATION_CHANGED_EVENT))
}

export function subscribeTaskCompletionNotification(
  listener: () => void
): () => void {
  if (typeof window === "undefined") return () => {}

  window.addEventListener("storage", listener)
  window.addEventListener(TASK_COMPLETION_NOTIFICATION_CHANGED_EVENT, listener)
  return () => {
    window.removeEventListener("storage", listener)
    window.removeEventListener(
      TASK_COMPLETION_NOTIFICATION_CHANGED_EVENT,
      listener
    )
  }
}
