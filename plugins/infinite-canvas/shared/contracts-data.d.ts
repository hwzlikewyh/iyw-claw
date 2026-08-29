export type ContractData = {
  schemaPath: string
  description: string
  readOnlyHint: boolean
  inputSchema: Record<string, unknown>
}

export const contracts: Readonly<Record<string, ContractData>>
export const resourceUri: string
