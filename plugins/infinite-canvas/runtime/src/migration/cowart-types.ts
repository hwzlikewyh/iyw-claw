export type CowartRecord = {
  id: string
  typeName?: string
  type?: string
  parentId?: string
  x?: number
  y?: number
  rotation?: number
  props?: Record<string, unknown>
  meta?: Record<string, unknown>
}

export type CowartPage = { pageId: string; sourcePath: string; sourceDirectory: string; sourceSha256: string; records: CowartRecord[]; warnings: string[] }
export type MigrationUnsupportedRecord = { id: string; type: string; reason: string }
export type MigrationReport = { schemaVersion: 1; pageId: string; targetCanvasId: string; dryRun: boolean; sourcePath: string; sourceSha256: string; mapped: number; skipped: number; warnings: string[]; unsupportedRecords?: MigrationUnsupportedRecord[]; reportPath?: string }
