import type { AssetClient, AssetReference } from "../asset-client.js"

export function createFileStorageAdapter(client: AssetClient) {
  return {
    saveFile: (file: Blob, name: string) => client.upload(file, name, file.type || "application/octet-stream"),
    importFile: (sourcePath: string, name: string, mimeType: string) => client.importSource(sourcePath, name, mimeType),
    getFileUrl: (file: AssetReference) => client.getUrl(file),
    deleteFile: (_file: AssetReference) => Promise.resolve(),
  }
}
