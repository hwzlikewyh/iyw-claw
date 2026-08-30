import { contracts, resourceUri } from "../../shared/contracts-data.mjs"

export { contracts, resourceUri }
export type ContractName = keyof typeof contracts
export type Contract = (typeof contracts)[ContractName]
