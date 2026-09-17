import { getShellTransport } from "@/lib/transport"

const REPORT_TIMEOUT_MS = 13 * 60 * 1000

export interface LogReportContext {
  today: string
  date: string
  logsDir: string
  sourceFiles: { name: string; sizeBytes: number }[]
}

export interface LogReportResult {
  fileUrl: string
  date: string
  sizeBytes: number
  logRecords: number
  skippedRecords: number
}

export function getLogReportContext(date?: string) {
  return getShellTransport().call<LogReportContext>("get_log_report_context", {
    date: date ?? null,
  })
}

export function submitLogReport(request: {
  date: string
  description: string
  screenshots: string[]
}) {
  return getShellTransport().call<LogReportResult>(
    "submit_log_report",
    { request },
    { timeoutMs: REPORT_TIMEOUT_MS }
  )
}

export function encodeReportScreenshot(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(new Error("Failed to read screenshot"))
    reader.onload = () => {
      const value = reader.result
      if (typeof value !== "string" || !value.includes(",")) {
        reject(new Error("Invalid screenshot data"))
        return
      }
      resolve(value.slice(value.indexOf(",") + 1))
    }
    reader.readAsDataURL(file)
  })
}
