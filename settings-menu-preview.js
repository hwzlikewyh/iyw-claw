const pages = window.settingsPreviewPages;
const groups = [
  { title: "外观与输入", pages: ["interface", "sessions"] },
  { title: "AI 工作区", pages: ["agents", "capabilities"] },
  { title: "连接与维护", pages: ["connections", "maintenance"] },
  { title: "统计", pages: ["insights"] },
];

const nav = document.querySelector("#settings-nav");
const search = document.querySelector("#nav-search");
const searchEmpty = document.querySelector("#search-empty");
const pageTitle = document.querySelector("#page-title");
const pageIntro = document.querySelector("#page-intro");
const appearancePage = document.querySelector("#appearance-page");
const fontPage = document.querySelector("#font-page");
const dynamicPages = document.querySelector("#dynamic-pages");
const sectionLinks = document.querySelector("#section-links");
const status = document.querySelector("#preview-status");
const navButtons = new Map();

function makeIcon(id) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  svg.setAttribute("class", "icon");
  svg.setAttribute("aria-hidden", "true");
  use.setAttribute("href", `#${id}`);
  svg.append(use);
  return svg;
}

function renderNavigation() {
  groups.forEach((group) => {
    const section = document.createElement("section");
    const heading = document.createElement("h2");
    section.className = "nav-group";
    heading.className = "group-title";
    heading.textContent = group.title;
    section.append(heading);
    group.pages.forEach((id) => {
      const page = pages.find((item) => item.id === id);
      const button = document.createElement("button");
      const label = document.createElement("span");
      button.className = "nav-item";
      button.type = "button";
      button.dataset.page = id;
      button.dataset.label = page.label;
      button.dataset.search = `${page.label} ${page.search}`.toLocaleLowerCase();
      button.setAttribute("aria-current", String(id === "interface"));
      button.append(makeIcon(page.icon));
      label.textContent = page.label;
      button.append(label);
      button.addEventListener("click", () => selectPage(id));
      navButtons.set(id, button);
      section.append(button);
    });
    nav.append(section);
  });
}

function renderSwitch(item, key) {
  const label = document.createElement("label");
  const input = document.createElement("input");
  const track = document.createElement("span");
  const text = document.createElement("span");
  label.className = "toggle";
  input.type = "checkbox";
  input.checked = Boolean(item.checked);
  input.setAttribute("aria-label", item.label);
  input.dataset.setting = key;
  track.className = "switch";
  text.textContent = item.checked ? "开启" : "关闭";
  input.addEventListener("change", () => {
    text.textContent = input.checked ? "开启" : "关闭";
    announce(`${item.label}：${text.textContent}`);
  });
  label.append(input, track, text);
  return label;
}

function renderSelect(item, key) {
  const select = document.createElement("select");
  select.setAttribute("aria-label", item.label);
  select.dataset.setting = key;
  item.options.forEach((option) => {
    const node = document.createElement("option");
    node.textContent = option;
    node.selected = option === item.value;
    select.append(node);
  });
  select.addEventListener("change", () => announce(`${item.label}：${select.value}`));
  return select;
}

function renderInput(item, key) {
  const controls = document.createElement("div");
  const input = document.createElement("input");
  controls.className = "controls";
  input.type = item.type;
  input.value = item.value ?? "";
  input.placeholder = item.placeholder ?? "";
  input.setAttribute("aria-label", item.label);
  input.dataset.setting = key;
  if (item.min !== undefined) input.min = item.min;
  if (item.max !== undefined) input.max = item.max;
  if (item.type === "number") input.className = "size-select";
  input.addEventListener("change", () => announce(`${item.label} 已更新预览`));
  controls.append(input);
  if (item.suffix) {
    const suffix = document.createElement("span");
    suffix.className = "read-value";
    suffix.textContent = item.suffix;
    controls.append(suffix);
  }
  return controls;
}

function renderAction(item) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "action";
  button.textContent = item.button;
  button.addEventListener("click", () => announce(`${item.label}：预览操作`));
  return button;
}

function renderControl(item, key) {
  const controls = document.createElement("div");
  controls.className = "controls";
  if (item.type === "switch") controls.append(renderSwitch(item, key));
  else if (item.type === "select") controls.append(renderSelect(item, key));
  else if (item.type === "number" || item.type === "text") return renderInput(item, key);
  else if (item.type === "button") controls.append(renderAction(item));
  else {
    const value = document.createElement("span");
    value.className = "read-value";
    value.textContent = item.value;
    controls.append(value);
  }
  return controls;
}

function makeSetting(item, pageId, index) {
  const row = document.createElement("div");
  const copy = document.createElement("div");
  const title = document.createElement("div");
  row.className = "setting-row functional-row";
  copy.className = "setting-copy";
  title.className = "setting-name";
  title.textContent = item.label;
  copy.append(title);
  if (item.detail) {
    const detail = document.createElement("p");
    detail.className = "setting-detail";
    detail.textContent = item.detail;
    copy.append(detail);
  }
  row.append(copy, renderControl(item, `${pageId}-${index}`));
  return row;
}

function makeSection(section, pageId, index) {
  const panel = document.createElement("section");
  const heading = document.createElement("div");
  const title = document.createElement("h2");
  panel.className = "section dynamic-section";
  panel.id = `${pageId}-section-${index}`;
  heading.className = "section-heading";
  title.textContent = section.title;
  heading.append(title);
  panel.append(heading);
  section.items.forEach((item, itemIndex) => panel.append(makeSetting(item, pageId, `${index}-${itemIndex}`)));
  return panel;
}

function renderSections(page) {
  const links = document.createDocumentFragment();
  const content = document.createDocumentFragment();
  page.sections.forEach((section, index) => {
    const detail = typeof section === "string" ? { title: section } : section;
    const anchor = document.createElement("a");
    anchor.href = `#${detail.target ?? `${page.id}-section-${index}`}`;
    anchor.textContent = detail.title;
    links.append(anchor);
    if (detail.items) content.append(makeSection(detail, page.id, index));
  });
  sectionLinks.replaceChildren(links);
  dynamicPages.replaceChildren(content);
}

function selectPage(id) {
  const page = pages.find((item) => item.id === id);
  if (!page) return;
  pageTitle.textContent = page.label;
  pageIntro.textContent = page.intro;
  appearancePage.hidden = id !== "interface";
  fontPage.hidden = id !== "interface";
  dynamicPages.hidden = id === "interface";
  sectionLinks.hidden = false;
  renderSections(page);
  navButtons.forEach((button, key) => button.setAttribute("aria-current", String(key === id)));
  document.querySelector("body").classList.remove("nav-open");
  document.querySelector(".canvas").scrollTop = 0;
}

function filterNavigation(query) {
  let matches = 0;
  document.querySelectorAll(".nav-group").forEach((group) => {
    let groupMatches = 0;
    group.querySelectorAll(".nav-item").forEach((button) => {
      const visible = button.dataset.search.includes(query);
      button.hidden = !visible;
      if (visible) groupMatches += 1;
    });
    group.hidden = groupMatches === 0;
    matches += groupMatches;
  });
  searchEmpty.hidden = matches > 0;
}

function announce(message) {
  status.textContent = `${message}。预览值不会保存到应用。`;
}

function bindFontPreview() {
  const fontSets = [
    { select: "#ui-font", custom: "#ui-custom", sample: "#ui-sample", fallback: "sans-serif" },
    { select: "#editor-font", custom: "#editor-custom", sample: "#editor-sample", size: "#editor-size", fallback: "monospace", ligature: "#editor-ligatures" },
    { select: "#terminal-font", custom: "#terminal-custom", sample: "#terminal-sample", size: "#terminal-size", fallback: "monospace", ligature: "#terminal-ligatures" },
  ];
  fontSets.forEach((config) => {
    const select = document.querySelector(config.select);
    const sample = document.querySelector(config.sample);
    const customGroup = document.querySelector(config.custom);
    const input = customGroup?.querySelector("input") ?? customGroup;
    const size = config.size ? document.querySelector(config.size) : null;
    const update = () => {
      const isCustom = select.value === "custom";
      if (customGroup) customGroup.hidden = !isCustom;
      const family = isCustom ? input.value.trim() : select.value;
      sample.style.fontFamily = family ? `"${family}", ${config.fallback}` : config.fallback;
      if (size) sample.style.fontSize = size.value;
      if (config.ligature) {
        const available = select.selectedOptions[0].dataset.ligatures === "true";
        const toggle = document.querySelector(config.ligature);
        toggle.disabled = !available;
        toggle.nextElementSibling.nextElementSibling.textContent = available ? "启用连字" : "该字体不含连字";
      }
    };
    select.addEventListener("change", update);
    input?.addEventListener("input", update);
    size?.addEventListener("change", update);
    update();
  });
  document.querySelector("#word-wrap").addEventListener("change", (event) => {
    document.querySelector("#editor-sample").style.whiteSpace = event.target.checked ? "pre-wrap" : "pre";
  });
}

renderNavigation();
bindFontPreview();
const requestedPage = new URL(window.location.href).searchParams.get("page");
selectPage(pages.some((page) => page.id === requestedPage) ? requestedPage : "interface");
search.addEventListener("input", () => filterNavigation(search.value.trim().toLocaleLowerCase()));
document.querySelector("#open-menu").addEventListener("click", () => document.body.classList.add("nav-open"));
document.querySelector("#close-menu").addEventListener("click", () => document.body.classList.remove("nav-open"));
document.querySelectorAll("[data-theme-mode]").forEach((button) => button.addEventListener("click", () => {
  document.querySelectorAll("[data-theme-mode]").forEach((mode) => mode.setAttribute("aria-pressed", String(mode === button)));
  announce(`主题模式：${button.textContent.trim()}`);
}));
document.querySelectorAll(".swatch").forEach((button) => button.addEventListener("click", () => {
  document.querySelectorAll(".swatch").forEach((swatch) => {
    swatch.setAttribute("aria-pressed", String(swatch === button));
    swatch.replaceChildren();
  });
  button.innerHTML = '<svg class="icon"><use href="#i-check"></use></svg>';
  announce(`主题颜色：${button.getAttribute("aria-label")}`);
}));
