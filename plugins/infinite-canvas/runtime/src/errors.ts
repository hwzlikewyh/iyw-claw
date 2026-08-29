export type RuntimeErrorCode =
  | "invalid_input"
  | "path_not_allowed"
  | "scene_invalid"
  | "revision_conflict"
  | "asset_not_found"
  | "asset_upload_invalid"
  | "asset_upload_incomplete"
  | "asset_hash_mismatch"
  | "runtime_unavailable"
  | "resource_not_found"
  | "migration_target_exists"

export class CanvasRuntimeError extends Error {
  readonly code: RuntimeErrorCode
  readonly details?: Record<string, unknown>

  constructor(code: RuntimeErrorCode, message: string, details?: Record<string, unknown>) {
    super(message)
    this.name = "CanvasRuntimeError"
    this.code = code
    this.details = details
  }

  toJSON() {
    return { code: this.code, message: this.message, ...(this.details ? { details: this.details } : {}) }
  }
}

export function invalid(message: string, details?: Record<string, unknown>): CanvasRuntimeError {
  return new CanvasRuntimeError("invalid_input", message, details)
}
