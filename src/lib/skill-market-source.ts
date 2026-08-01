import type {
  SkillMarketAddVersionRequest,
  SkillMarketCategory,
  SkillMarketInstallErrorCode,
  SkillMarketInstallPlanV2,
  SkillMarketListQueryV2,
  SkillMarketPublishRequest,
  SkillMarketV2CatalogPage,
  SkillMarketV2Detail,
  SkillMarketV2FileNode,
  SkillMarketV2Item,
  SkillMarketV2Version,
  SkillMarketAudience,
} from "@/lib/skill-market"
import { createFixtureSkillMarketSource } from "@/lib/skill-market-fixtures"

// ---------------------------------------------------------------------------
// Skill Market data source seam (Task 10)
//
// The UI only talks to `SkillMarketSource`. Today the factory returns the
// typed fixture implementation (contract v2 is not frozen yet; Task 01/03
// blocked). Once contract_revision is frozen, Task 13 wires a transport-backed
// implementation here without touching the UI.
// ---------------------------------------------------------------------------

export interface SkillMarketPublishRequestV2 {
  slug: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  audience: SkillMarketAudience
  version: string
  changelog: string
  dependencies: SkillMarketPublishRequest["dependencies"]
  files: SkillMarketPublishRequest["files"]
}

export interface SkillMarketMetadataRequestV2 {
  id: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  audience: SkillMarketAudience
}

export interface SkillMarketAddVersionRequestV2 {
  id: string
  version: string
  changelog: string
  dependencies: SkillMarketAddVersionRequest["dependencies"]
  files: SkillMarketAddVersionRequest["files"]
}

export class SkillMarketSourceError extends Error {
  readonly code: SkillMarketInstallErrorCode

  constructor(code: SkillMarketInstallErrorCode, message?: string) {
    super(message ?? code)
    this.name = "SkillMarketSourceError"
    this.code = code
  }
}

export interface SkillMarketSource {
  list(query: SkillMarketListQueryV2): Promise<SkillMarketV2CatalogPage>
  categories(): Promise<SkillMarketCategory[]>
  detail(id: string, version?: string | null): Promise<SkillMarketV2Detail>
  versions(id: string): Promise<SkillMarketV2Version[]>
  files(id: string, version: string): Promise<SkillMarketV2FileNode[]>
  resolve(id: string, version: string): Promise<SkillMarketInstallPlanV2>
  publish(request: SkillMarketPublishRequestV2): Promise<SkillMarketV2Item>
  addVersion(
    request: SkillMarketAddVersionRequestV2
  ): Promise<SkillMarketV2Item>
  updateMetadata(
    request: SkillMarketMetadataRequestV2
  ): Promise<SkillMarketV2Item>
  delete(id: string): Promise<void>
  uninstall(id: string): Promise<void>
  rebuildArtifact(id: string, version: string): Promise<SkillMarketV2Version>
}

export interface SkillMarketSourceOptions {
  /** Number of synthetic catalog entries for list benchmarks (500/5000). */
  perfCount?: number
}

/**
 * Returns the active data source. Fixtures are the default until the frozen
 * contract v2 lands; Task 13 swaps this to the transport-backed source.
 */
export function getSkillMarketSource(
  options?: SkillMarketSourceOptions
): SkillMarketSource {
  return createFixtureSkillMarketSource(options)
}
