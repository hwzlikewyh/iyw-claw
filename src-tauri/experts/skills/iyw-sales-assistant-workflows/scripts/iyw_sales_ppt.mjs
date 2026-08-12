import fs from "node:fs/promises";
import path from "node:path";

import { Presentation, PresentationFile } from "@oai/artifact-tool";
import {
  ACCENT,
  HEIGHT,
  INK,
  MUTED,
  PALE,
  WARM,
  WIDTH,
  addImage,
  addTitle,
  box,
  clip,
  setSources,
} from "./iyw_sales_ppt_theme.mjs";

function parseArgs(argv) {
  const result = {};
  for (let index = 0; index < argv.length; index += 2) {
    result[String(argv[index] || "").replace(/^--/, "")] = argv[index + 1];
  }
  return result;
}

function addCover(presentation, input) {
  const slide = presentation.slides.add();
  slide.background.fill = PALE;
  slide.shapes.add({
    geometry: "rect",
    name: "cover-accent",
    position: { left: 0, top: 0, width: 18, height: HEIGHT },
    fill: ACCENT,
    line: { style: "solid", fill: ACCENT, width: 0 },
  });
  box(slide, "cover-eyebrow", "IYW SALES ASSISTANT", { left: 58, top: 52, width: 460, height: 30 }, {
    fontSize: 22,
    bold: true,
    color: ACCENT,
  });
  box(slide, "cover-title", clip(input.company_name, 36), { left: 58, top: 192, width: 1020, height: 180 }, {
    fontSize: 68,
    bold: true,
    verticalAlignment: "bottom",
  });
  box(slide, "cover-subtitle", "企业销售资料", { left: 58, top: 402, width: 400, height: 42 }, {
    fontSize: 32,
    color: WARM,
    bold: true,
  });
  box(slide, "cover-market", clip(input.market_keywords, 90), { left: 58, top: 470, width: 800, height: 60 }, {
    fontSize: 22,
    color: MUTED,
  });
  box(slide, "cover-date", input.as_of || "", { left: 58, top: 632, width: 250, height: 24 }, {
    fontSize: 14,
    color: MUTED,
  });
  setSources(slide, [input.shop_url]);
}

function addOverview(presentation, input) {
  const slide = presentation.slides.add();
  slide.background.fill = "#FFFFFF";
  addTitle(slide, "先看清企业，再决定切入点", 2);
  box(slide, "business-label", "工商信息", { left: 52, top: 152, width: 180, height: 42 }, {
    fontSize: 32,
    bold: true,
    color: ACCENT,
  });
  box(slide, "business-info", clip(input.business_info, 520), { left: 52, top: 204, width: 530, height: 410 }, {
    fontSize: 22,
    color: INK,
  });
  box(slide, "contact-label", "优先联系人", { left: 650, top: 152, width: 220, height: 42 }, {
    fontSize: 32,
    bold: true,
    color: WARM,
  });
  const contacts = input.contacts.length
    ? input.contacts.map((item) => `${item.name} / ${item.role}\n${item.phone}`).join("\n\n")
    : "联系人待补";
  box(slide, "contacts", clip(contacts, 260), { left: 650, top: 204, width: 530, height: 170 }, {
    fontSize: 22,
  });
  box(slide, "store-label", "店铺与商品", { left: 650, top: 414, width: 220, height: 42 }, {
    fontSize: 32,
    bold: true,
    color: ACCENT,
  });
  const links = [input.shop_url, ...input.product_urls].filter(Boolean).join("\n");
  box(slide, "store-links", clip(links, 300), { left: 650, top: 466, width: 530, height: 150 }, {
    fontSize: 22,
    color: MUTED,
  });
  setSources(slide, [input.company_source, input.shop_url, ...input.contacts.map((item) => item.source)]);
}

async function addProducts(presentation, input) {
  const slide = presentation.slides.add();
  slide.background.fill = "#FFFFFF";
  addTitle(slide, "产品图显示的是店铺当前主推方向", 3);
  box(slide, "product-summary-label", "产品判断", { left: 52, top: 160, width: 180, height: 42 }, {
    fontSize: 32,
    bold: true,
    color: WARM,
  });
  const product = input.products[0] || {};
  const summary = `${product.name || "店铺产品"}\n\n${product.summary || "暂无图片分析"}\n\n销售切入：${product.angle || input.sales_angle}`;
  box(slide, "product-summary", clip(summary, 360), { left: 52, top: 216, width: 430, height: 390 }, {
    fontSize: 22,
  });
  const frames = [
    { left: 540, top: 160, width: 310, height: 210 },
    { left: 880, top: 160, width: 310, height: 210 },
    { left: 710, top: 400, width: 310, height: 210 },
  ];
  let added = 0;
  for (let index = 0; index < Math.min(3, input.product_images.length); index += 1) {
    added += Number(await addImage(slide, input.product_images[index], frames[index], `店铺产品图 ${index + 1}`));
  }
  if (!added) {
    box(slide, "product-image-empty", "暂无可用产品图", { left: 610, top: 300, width: 500, height: 80 }, {
      fontSize: 32,
      color: MUTED,
      alignment: "center",
    });
  }
  setSources(slide, input.product_images.map((item) => item.source));
}

function addActivities(presentation, input) {
  const slide = presentation.slides.add();
  slide.background.fill = PALE;
  addTitle(slide, "近半年证据决定触达优先级", 4);
  slide.shapes.add({
    geometry: "line",
    name: "activity-line",
    position: { left: 110, top: 318, width: 1020, height: 0 },
    fill: "none",
    line: { style: "solid", fill: MUTED, width: 2 },
  });
  const groups = ["招聘", "知识产权", "参展"];
  const xPositions = [170, 570, 970];
  groups.forEach((group, index) => {
    slide.shapes.add({
      geometry: "ellipse",
      name: `activity-node-${index}`,
      position: { left: xPositions[index], top: 306, width: 24, height: 24 },
      fill: index === 1 ? WARM : ACCENT,
      line: { style: "solid", fill: "#FFFFFF", width: 2 },
    });
    box(slide, `activity-label-${index}`, group, { left: xPositions[index] - 30, top: 244, width: 180, height: 42 }, {
      fontSize: 32,
      bold: true,
    });
    const detail = (input.activities[group] || ["无有效记录"]).slice(0, 3).join("\n");
    box(slide, `activity-detail-${index}`, clip(detail, 240), { left: xPositions[index] - 70, top: 368, width: 300, height: 190 }, {
      fontSize: 22,
      color: MUTED,
    });
  });
  setSources(slide, input.activity_sources || []);
}

function isMaterialImage(item) {
  return /\.(gif|jpe?g|png|webp)$/i.test(String(item.local_path || ""));
}

function materialSummary(input) {
  const types = [...new Set(input.materials.map((item) => item.type))];
  const nonImages = input.materials.filter((item) => !isMaterialImage(item));
  const images = input.materials.filter(isMaterialImage);
  const listed = [...nonImages, ...images.slice(0, 4)];
  const files = listed.map((item) => {
    const name = clip(item.name || "未命名资料", 24);
    const source = clip(item.source || "资料来源未提供", 26);
    return `- [${item.type || "销售资料"}] ${name} | ${source}`;
  });
  const inventory = files.length ? `\n\n已取得文件：\n${files.join("\n")}` : "";
  const countNote = images.length > 4 ? `\n图片资料共 ${images.length} 份，展示 4 张代表图` : "";
  return `市场：${input.market_keywords}\n\n资料类型：${types.join("、") || "资料待补"}${countNote}${inventory}`;
}

async function addMaterials(presentation, input) {
  const slide = presentation.slides.add();
  slide.background.fill = "#FFFFFF";
  addTitle(slide, "资料已按市场用途整理", 5);
  box(slide, "material-types", materialSummary(input), { left: 52, top: 155, width: 370, height: 475 }, {
    fontSize: input.materials.length > 8 ? 16 : 18,
  });
  const imageMaterials = input.materials.filter(isMaterialImage);
  const frames = [
    { left: 470, top: 160, width: 330, height: 205 },
    { left: 840, top: 160, width: 330, height: 205 },
    { left: 470, top: 410, width: 330, height: 205 },
    { left: 840, top: 410, width: 330, height: 205 },
  ];
  let added = 0;
  for (let index = 0; index < Math.min(4, imageMaterials.length); index += 1) {
    const item = imageMaterials[index];
    const ok = await addImage(slide, item, frames[index], item.name || `销售资料 ${index + 1}`);
    if (ok) {
      added += 1;
      box(slide, `material-caption-${index}`, clip(item.name, 24), { left: frames[index].left, top: frames[index].top + 210, width: frames[index].width, height: 30 }, {
        fontSize: 22,
        color: MUTED,
      });
    }
  }
  if (!added) {
    const status = input.materials.length ? "已取得非图片资料\n详见左侧清单" : "图片工作流资料待补";
    box(slide, "material-empty", status, { left: 570, top: 285, width: 500, height: 110 }, {
      fontSize: 32,
      color: MUTED,
      alignment: "center",
    });
  }
  setSources(slide, input.materials.map((item) => item.source));
}

function addOpening(presentation, input) {
  const slide = presentation.slides.add();
  slide.background.fill = PALE;
  box(slide, "opening-eyebrow", "建议首轮触达", { left: 52, top: 48, width: 300, height: 42 }, {
    fontSize: 32,
    bold: true,
    color: ACCENT,
  });
  box(slide, "opening-title", "开场先谈对方的产品与市场", { left: 52, top: 132, width: 980, height: 70 }, {
    fontSize: 48,
    bold: true,
  });
  box(slide, "opening-copy", clip(input.opening_copy, 300), { left: 52, top: 252, width: 1100, height: 250 }, {
    fontSize: 28,
    color: INK,
    verticalAlignment: "center",
  });
  slide.shapes.add({
    geometry: "line",
    name: "opening-rule",
    position: { left: 52, top: 548, width: 1100, height: 0 },
    fill: "none",
    line: { style: "solid", fill: WARM, width: 3 },
  });
  box(slide, "opening-angle", clip(`切入建议：${input.sales_angle}`, 180), { left: 52, top: 580, width: 1100, height: 58 }, {
    fontSize: 22,
    color: MUTED,
  });
  setSources(slide, [input.shop_url]);
}

async function writePreview(presentation, workspace) {
  for (const [index, slide] of presentation.slides.items.entries()) {
    const blob = await presentation.export({ slide, format: "png", scale: 1 });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    await fs.writeFile(path.join(workspace, `slide-${index + 1}.png`), bytes);
  }
}

export async function buildCompanyPresentation(input, output, workspace) {
  const presentation = Presentation.create({ slideSize: { width: WIDTH, height: HEIGHT } });
  addCover(presentation, input);
  addOverview(presentation, input);
  await addProducts(presentation, input);
  addActivities(presentation, input);
  await addMaterials(presentation, input);
  addOpening(presentation, input);
  await writePreview(presentation, workspace);
  const sources = input.materials.map((item) => `${item.name}: ${item.source}`).join("\n");
  await fs.writeFile(path.join(workspace, "source-notes.txt"), sources || "输入记录", "utf8");
  const pptx = await PresentationFile.exportPptx(presentation);
  await pptx.save(output);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.input || !args.output || !args.workspace) {
    throw new Error("Usage: node iyw_sales_ppt.mjs --input <json> --output <pptx> --workspace <dir>");
  }
  const input = JSON.parse(await fs.readFile(path.resolve(args.input), "utf8"));
  await fs.mkdir(path.dirname(path.resolve(args.output)), { recursive: true });
  await buildCompanyPresentation(input, path.resolve(args.output), path.resolve(args.workspace));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
