import i18n from "@/i18n"

export class ImageDownloadError extends Error {
  constructor(readonly sourceUrl: string) {
    super(i18n.t("imageDownload.failed"))
    this.name = "ImageDownloadError"
  }
}

export function imageRecovery(error: unknown): { recoverySourceUrl?: string } {
  return error instanceof ImageDownloadError ? { recoverySourceUrl: error.sourceUrl } : {}
}

i18n.addResourceBundle("zh-CN", "translation", { imageDownload: {
  failed: "图片已生成，但下载或保存失败。点击“重试下载”即可恢复，不会重新生成。",
  retry: "重试下载", title: "图片下载失败", sourceMissing: "旧版本未保留结果链接，无法单独恢复下载，请勿反复重试生成。",
} }, true, true)
i18n.addResourceBundle("en-US", "translation", { imageDownload: {
  failed: "The image was generated, but could not be downloaded or saved. Retry download to recover it without generating again.",
  retry: "Retry download", title: "Image download failed", sourceMissing: "The previous version did not retain the result URL. Download cannot be resumed; do not repeatedly regenerate.",
} }, true, true)
