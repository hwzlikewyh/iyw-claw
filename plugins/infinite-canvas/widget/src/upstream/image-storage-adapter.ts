import type { AssetClient, AssetReference } from "../asset-client.js"

export type StoredImage = AssetReference & { naturalWidth: number; naturalHeight: number }

export function createImageStorageAdapter(client: AssetClient) {
  return {
    saveImageFile: (file: Blob, name: string) => client.upload(file, name, file.type || "image/png"),
    getImageUrl: (image: StoredImage) => client.getUrl(image),
    deleteImageFile: (_image: StoredImage) => Promise.resolve(),
  }
}
