import fs from "node:fs/promises";
import path from "node:path";

export const WIDTH = 1280;
export const HEIGHT = 720;
export const FONT = "Microsoft YaHei";
export const INK = "#17212B";
export const MUTED = "#5E6B78";
export const ACCENT = "#0E7490";
export const WARM = "#D97706";
export const PALE = "#F4F7F8";
export const RULE = "#D8E0E4";

export function clip(value, limit = 180) {
  const text = String(value || "").trim();
  return text.length <= limit ? text : `${text.slice(0, limit - 1)}...`;
}

export function box(slide, name, text, position, style = {}) {
  const shape = slide.shapes.add({
    geometry: "textbox",
    name,
    position,
    fill: style.fill || "none",
    line: style.line || { style: "solid", fill: "none", width: 0 },
    borderRadius: style.borderRadius,
  });
  shape.text = text;
  shape.text.style = {
    typeface: FONT,
    fontSize: style.fontSize || 22,
    color: style.color || INK,
    bold: Boolean(style.bold),
    alignment: style.alignment || "left",
    verticalAlignment: style.verticalAlignment || "top",
  };
  return shape;
}

export function addTitle(slide, title, number) {
  box(slide, `title-${number}`, clip(title, 36), { left: 52, top: 40, width: 1110, height: 66 }, {
    fontSize: 48,
    bold: true,
  });
  box(slide, `page-${number}`, String(number).padStart(2, "0"), { left: 1182, top: 52, width: 50, height: 24 }, {
    fontSize: 14,
    color: MUTED,
    alignment: "right",
  });
  slide.shapes.add({
    geometry: "line",
    name: `title-rule-${number}`,
    position: { left: 52, top: 118, width: 1180, height: 0 },
    fill: "none",
    line: { style: "solid", fill: RULE, width: 1 },
  });
}

export function setSources(slide, sources) {
  const unique = [...new Set(sources.map((item) => String(item || "").trim()).filter(Boolean))];
  const lines = unique.length ? unique.map((item) => `- ${item}`) : ["- 输入记录"];
  slide.speakerNotes.textFrame.setText(`[Sources]\n${lines.join("\n")}`);
  slide.speakerNotes.setVisible(true);
}

function contentType(filePath) {
  const extension = path.extname(filePath).toLowerCase();
  const types = {
    ".gif": "image/gif",
    ".jpeg": "image/jpeg",
    ".jpg": "image/jpeg",
    ".png": "image/png",
    ".webp": "image/webp",
  };
  return types[extension];
}

export async function addImage(slide, item, position, alt) {
  try {
    const type = contentType(item.local_path);
    if (!type) return false;
    const bytes = await fs.readFile(item.local_path);
    slide.images.add({
      blob: bytes,
      contentType: type,
      alt,
      fit: "cover",
      geometry: "roundRect",
      borderRadius: 8,
      position,
    });
    return true;
  } catch {
    return false;
  }
}
