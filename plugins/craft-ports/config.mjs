export function craftConfig(upstream) {
  if (upstream.version !== "0.2.4")
    throw Error("Unexpected Craft source version")
  if (upstream.name === "photocraft")
    return {
      source: "photocraft",
      name: "iyw-image-editor",
      title: "PhotoCraft",
      skill: "image-editor",
      photo: true,
    }
  if (upstream.name === "pdfcraft")
    return {
      source: "pdfcraft",
      name: "iyw-pdfcraft",
      title: "PdfCraft",
      skill: "pdfcraft",
      photo: false,
    }
  throw Error("Only PhotoCraft and PdfCraft are supported")
}
