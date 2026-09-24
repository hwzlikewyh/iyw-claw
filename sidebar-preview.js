const ICONS = {
"ChartNoAxesCombined": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-chart-no-axes-combined\" aria-hidden=\"true\"><path d=\"M12 16v5\"></path><path d=\"M16 14v7\"></path><path d=\"M20 10v11\"></path><path d=\"m22 3-8.646 8.646a.5.5 0 0 1-.708 0L9.354 8.354a.5.5 0 0 0-.707 0L2 15\"></path><path d=\"M4 18v3\"></path><path d=\"M8 14v7\"></path></svg>",
"ListTodo": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-list-todo\" aria-hidden=\"true\"><path d=\"M13 5h8\"></path><path d=\"M13 12h8\"></path><path d=\"M13 19h8\"></path><path d=\"m3 17 2 2 4-4\"></path><rect x=\"3\" y=\"4\" width=\"6\" height=\"6\" rx=\"1\"></rect></svg>",
"Settings": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-settings\" aria-hidden=\"true\"><path d=\"M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915\"></path><circle cx=\"12\" cy=\"12\" r=\"3\"></circle></svg>",
"Trash2": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-trash2 lucide-trash-2\" aria-hidden=\"true\"><path d=\"M10 11v6\"></path><path d=\"M14 11v6\"></path><path d=\"M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6\"></path><path d=\"M3 6h18\"></path><path d=\"M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2\"></path></svg>",
  "PanelLeftClose": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-panel-left-close\" aria-hidden=\"true\"><rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"></rect><path d=\"M9 3v18\"></path><path d=\"m16 15-3-3 3-3\"></path></svg>",
  "PanelLeftOpen": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-panel-left-open\" aria-hidden=\"true\"><rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"></rect><path d=\"M9 3v18\"></path><path d=\"m14 9 3 3-3 3\"></path></svg>",
  "SquarePen": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-square-pen\" aria-hidden=\"true\"><path d=\"M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7\"></path><path d=\"M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z\"></path></svg>",
  "Search": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-search\" aria-hidden=\"true\"><path d=\"m21 21-4.34-4.34\"></path><circle cx=\"11\" cy=\"11\" r=\"8\"></circle></svg>",
  "ChevronDown": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-chevron-down\" aria-hidden=\"true\"><path d=\"m6 9 6 6 6-6\"></path></svg>",
  "ChevronsUpDown": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-chevrons-up-down\" aria-hidden=\"true\"><path d=\"m7 15 5 5 5-5\"></path><path d=\"m7 9 5-5 5 5\"></path></svg>",
  "Plus": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-plus\" aria-hidden=\"true\"><path d=\"M5 12h14\"></path><path d=\"M12 5v14\"></path></svg>",
  "MessageSquare": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-message-square\" aria-hidden=\"true\"><path d=\"M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z\"></path></svg>",
  "Pin": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-pin\" aria-hidden=\"true\"><path d=\"M12 17v5\"></path><path d=\"M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z\"></path></svg>",
  "Folder": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-folder\" aria-hidden=\"true\"><path d=\"M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z\"></path></svg>",
  "CalendarClock": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-calendar-clock\" aria-hidden=\"true\"><path d=\"M16 14v2.2l1.6 1\"></path><path d=\"M16 2v4\"></path><path d=\"M21 7.5V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h3.5\"></path><path d=\"M3 10h5\"></path><path d=\"M8 2v4\"></path><circle cx=\"16\" cy=\"16\" r=\"6\"></circle></svg>",
  "Layers": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-layers\" aria-hidden=\"true\"><path d=\"M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z\"></path><path d=\"M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12\"></path><path d=\"M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17\"></path></svg>",
  "BookOpen": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-book-open\" aria-hidden=\"true\"><path d=\"M12 7v14\"></path><path d=\"M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z\"></path></svg>",
  "Settings2": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-settings2 lucide-settings-2\" aria-hidden=\"true\"><path d=\"M14 17H5\"></path><path d=\"M19 7h-9\"></path><circle cx=\"17\" cy=\"17\" r=\"3\"></circle><circle cx=\"7\" cy=\"7\" r=\"3\"></circle></svg>",
  "Coins": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-coins\" aria-hidden=\"true\"><circle cx=\"8\" cy=\"8\" r=\"6\"></circle><path d=\"M18.09 10.37A6 6 0 1 1 10.34 18\"></path><path d=\"M7 6h1v4\"></path><path d=\"m16.71 13.88.7.71-2.82 2.82\"></path></svg>",
  "ArrowUpRight": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-arrow-up-right\" aria-hidden=\"true\"><path d=\"M7 7h10v10\"></path><path d=\"M7 17 17 7\"></path></svg>",
  "Copy": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-copy\" aria-hidden=\"true\"><rect width=\"14\" height=\"14\" x=\"8\" y=\"8\" rx=\"2\" ry=\"2\"></rect><path d=\"M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2\"></path></svg>",
  "Brain": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-brain\" aria-hidden=\"true\"><path d=\"M12 18V5\"></path><path d=\"M15 13a4.17 4.17 0 0 1-3-4 4.17 4.17 0 0 1-3 4\"></path><path d=\"M17.598 6.5A3 3 0 1 0 12 5a3 3 0 1 0-5.598 1.5\"></path><path d=\"M17.997 5.125a4 4 0 0 1 2.526 5.77\"></path><path d=\"M18 18a4 4 0 0 0 2-7.464\"></path><path d=\"M19.967 17.483A4 4 0 1 1 12 18a4 4 0 1 1-7.967-.517\"></path><path d=\"M6 18a4 4 0 0 1-2-7.464\"></path><path d=\"M6.003 5.125a4 4 0 0 0-2.526 5.77\"></path></svg>",
  "Sun": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-sun\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"4\"></circle><path d=\"M12 2v2\"></path><path d=\"M12 20v2\"></path><path d=\"m4.93 4.93 1.41 1.41\"></path><path d=\"m17.66 17.66 1.41 1.41\"></path><path d=\"M2 12h2\"></path><path d=\"M20 12h2\"></path><path d=\"m6.34 17.66-1.41 1.41\"></path><path d=\"m19.07 4.93-1.41 1.41\"></path></svg>",
  "Moon": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-moon\" aria-hidden=\"true\"><path d=\"M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401\"></path></svg>",
  "Monitor": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-monitor\" aria-hidden=\"true\"><rect width=\"20\" height=\"14\" x=\"2\" y=\"3\" rx=\"2\"></rect><line x1=\"8\" x2=\"16\" y1=\"21\" y2=\"21\"></line><line x1=\"12\" x2=\"12\" y1=\"17\" y2=\"21\"></line></svg>",
  "RefreshCw": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-refresh-cw\" aria-hidden=\"true\"><path d=\"M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8\"></path><path d=\"M21 3v5h-5\"></path><path d=\"M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16\"></path><path d=\"M8 16H3v5\"></path></svg>",
  "LogOut": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-log-out\" aria-hidden=\"true\"><path d=\"m16 17 5-5-5-5\"></path><path d=\"M21 12H9\"></path><path d=\"M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4\"></path></svg>",
  "Check": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-check\" aria-hidden=\"true\"><path d=\"M20 6 9 17l-5-5\"></path></svg>",
  "X": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-x\" aria-hidden=\"true\"><path d=\"M18 6 6 18\"></path><path d=\"m6 6 12 12\"></path></svg>",
  "ArrowRight": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-arrow-right\" aria-hidden=\"true\"><path d=\"M5 12h14\"></path><path d=\"m12 5 7 7-7 7\"></path></svg>",
  "ListFilter": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" class=\"lucide lucide-list-filter\" aria-hidden=\"true\"><path d=\"M2 5h20\"></path><path d=\"M6 12h12\"></path><path d=\"M9 19h6\"></path></svg>"
}
const $ = (selector, root = document) => root.querySelector(selector)
const $$ = (selector, root = document) => [...root.querySelectorAll(selector)]
const icon = (name) => ICONS[name] || ''
const escapeHtml = (text) => String(text).replace(/[&<>"']/g, (char) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[char])
const STATUS = { all: '全部状态', running: '进行中', completed: '已完成', failed: '失败', pending: '待处理', planning: '规划中' }
const PERIODS = { all: '全部时间', today: '今天', week: '最近 7 天', month: '最近 30 天' }
const PERIOD_DAYS = { today: 1, week: 7, month: 30 }
const POPOVER_GAP = 8, MOBILE_WIDTH = 700, TOAST_DURATION = 2600
const data = [
  { id: 'nav', title: '修复首字耗时统计', status: 'running', age: 0, group: 'pinned', project: 'iyw-claw' },
  { id: 'release', title: '发布前回归检查', status: 'pending', age: 0, group: 'pinned', project: 'iyw-claw' },
  { id: 'token', title: '优化工作空间导航', status: 'completed', age: 0, group: 'claw', project: 'iyw-claw' },
  { id: 'upload', title: '上传工作区外文件', status: 'completed', age: 1, group: 'claw', project: 'iyw-claw' },
  { id: 'model', title: '检查模型偏好设置', status: 'failed', age: 4, group: 'claw', project: 'iyw-claw' },
  { id: 'gateway', title: '检查网关请求日志', status: 'running', age: 1, group: 'fusion', project: 'iyw-fusion-api' },
  { id: 'api', title: '整理接口文档', status: 'planning', age: 12, group: 'fusion', project: 'iyw-fusion-api' },
  { id: 'requirements', title: '整理需求与验收标准', status: 'completed', age: 0, group: 'recent', project: '个人空间' },
  { id: 'environment', title: '更新本地开发环境', status: 'completed', age: 1, group: 'recent', project: '个人空间' },
  { id: 'automation', title: '设计自动化任务入口', status: 'planning', age: 42, group: 'recent', project: '个人空间' }
]
const state = { status: 'all', time: 'all', active: 'nav', closed: new Set(['fusion']), popover: null, anchor: null, signedIn: true, theme: 'light' }
const collapsedSections = new Set()
const messages = new Map(); let toastTimer
function hydrateIcons(root = document) {
  $$('[data-icon]', root).forEach((node) => { const holder = document.createElement('span'); holder.innerHTML = icon(node.closest('.settings-trigger') ? 'Settings' : node.dataset.icon); const svg = holder.firstElementChild; if (svg) { svg.classList.add(...node.classList); node.replaceWith(svg) } })
}
function statusMarkup(item, withLabel = false) {
  const label = STATUS[item.status]
  return '<span class="conversation-status status-' + item.status + '" title="' + label + '" aria-label="' + label + '"><span class="dot"></span>' + (withLabel ? label : '') + '</span>'
}
function conversationRow(item, prominent = false) {
  const selected = item.id === state.active
  const meta = item.group === 'recent' ? '<span class="meta">' + (item.age === 0 ? '今天' : item.age === 1 ? '昨天' : item.age + ' 天前') + '</span>' : statusMarkup(item, prominent)
  return '<button class="conversation' + (selected ? ' active' : '') + (prominent ? ' pinned' : '') + '" data-conversation="' + item.id + '" title="' + escapeHtml(item.title) + '"' + (selected ? ' aria-current="page"' : '') + '><span class="title">' + escapeHtml(item.title) + (prominent ? '<small>' + item.project + ' · ' + (item.agent || 'Codex') + '</small>' : '') + '</span>' + meta + '</button>'
}
function projectRows(id, title, entries) {
  if (!entries.length) return ''
  const expanded = !state.closed.has(id)
  return '<section class="project"><button class="project-heading" data-project="' + id + '" aria-expanded="' + expanded + '">' + icon('ChevronDown') + icon('Folder') + '<span>' + title + '</span><span class="total">' + entries.length + '</span></button><div class="project-children"' + (expanded ? '' : ' hidden') + '>' + entries.map((item) => conversationRow(item)).join('') + '</div></section>'
}
function collapsibleSection(id, title, content) {
  const expanded = !collapsedSections.has(id)
  return '<section class="conversation-group"><button class="group-heading group-toggle" data-section-toggle="' + id + '" aria-expanded="' + expanded + '" aria-controls="section-' + id + '">' + icon('ChevronDown') + title + '</button><div id="section-' + id + '"' + (expanded ? '' : ' hidden') + '>' + content + '</div></section>'
}
function sectionRows(id, title, entries) {
  return entries.length ? collapsibleSection(id, title, entries.map((item) => conversationRow(item, id === 'pinned')).join('')) : ''
}
function renderList() {
  const filtering = state.status !== 'all' || state.time !== 'all'
  const entries = data.filter((item) => (state.status === 'all' || item.status === state.status) && (state.time === 'all' || item.age < PERIOD_DAYS[state.time]))
  const group = (name) => entries.filter((item) => item.group === name)
  const projects = projectRows('claw', 'iyw-claw', group('claw')) + projectRows('fusion', 'iyw-fusion-api', group('fusion'))
  $('#conversation-list').innerHTML = sectionRows('pinned', '置顶', group('pinned')) + (projects ? collapsibleSection('projects', '项目', projects) : '') + sectionRows('chats', '独立对话', group('recent')) || '<p class="empty-list">没有符合条件的会话</p>'
  $('#total').textContent = entries.length
  $('#filter-summary').hidden = !filtering
  $('#filter-summary > span').textContent = [state.status === 'all' ? '' : STATUS[state.status], state.time === 'all' ? '' : PERIODS[state.time]].filter(Boolean).join(' · ')
  $('#filter-trigger').classList.toggle('filtered', filtering)
  $('.filter-dot').hidden = !filtering
  $('#reset-filter').disabled = !filtering
}
function renderOptions() {
  for (const [name, options] of [['status', STATUS], ['time', PERIODS]]) {
    $('#' + name + '-options').innerHTML = Object.entries(options).map(([value, label]) => '<label class="filter-option"><input type="radio" name="' + name + '" value="' + value + '"' + (state[name] === value ? ' checked' : '') + ' />' + label + icon('Check') + '</label>').join('')
  }
}
function resetFilter() { state.status = 'all'; state.time = 'all'; renderOptions(); renderList() }
function positionPopover() {
  if (!state.popover) return
  const panel = state.popover
  const viewportWidth = document.documentElement.clientWidth
  const viewportHeight = window.innerHeight
  const anchor = state.anchor.getBoundingClientRect()
  panel.style.maxHeight = Math.max(0, viewportHeight - POPOVER_GAP * 2) + 'px'
  panel.style.maxWidth = Math.max(0, viewportWidth - POPOVER_GAP * 2) + 'px'
  const height = panel.offsetHeight
  const isAccount = panel.id === 'account-panel'
  const isProject = panel.id === 'project-panel'
  const preferredLeft = isAccount || isProject ? anchor.left : anchor.right - panel.offsetWidth
  const preferredTop = isAccount ? anchor.top - height - POPOVER_GAP : anchor.bottom + POPOVER_GAP
  panel.style.left = Math.max(POPOVER_GAP, Math.min(preferredLeft, viewportWidth - panel.offsetWidth - POPOVER_GAP)) + 'px'
  panel.style.top = Math.max(POPOVER_GAP, Math.min(preferredTop, viewportHeight - height - POPOVER_GAP)) + 'px'
}
function closePopover(restoreFocus = false) {
  if (!state.popover) return
  state.popover.hidden = true
  state.anchor.setAttribute('aria-expanded', 'false')
  if (restoreFocus) state.anchor.focus()
  state.popover = null
  state.anchor = null
}
function togglePopover(id, anchor, focus = true) {
  const same = state.popover?.id === id
  closePopover()
  if (same) return
  state.popover = $('#' + id); state.anchor = anchor; state.popover.hidden = false
  anchor.setAttribute('aria-expanded', 'true')
  positionPopover()
  if (focus) $('button, input', state.popover)?.focus({ preventScroll: true })
}
function toast(text) { clearTimeout(toastTimer); $('#toast').textContent = text; $('#toast').hidden = false; toastTimer = setTimeout(() => { $('#toast').hidden = true }, TOAST_DURATION) }
function setTheme(choice) {
  state.theme = choice
  document.documentElement.dataset.theme = choice === 'system' ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') : choice
  $$('[data-theme-choice]').forEach((button) => button.setAttribute('aria-pressed', String(button.dataset.themeChoice === choice)))
}
function toggleSidebar() {
  closePopover()
  const collapsed = $('#app').classList.toggle('collapsed')
  const label = collapsed ? '展开侧栏' : '收起侧栏'
  $('#toggle').setAttribute('aria-expanded', String(!collapsed)); $('#toggle').setAttribute('aria-label', label); $('#toggle').title = label
  $('#toggle').innerHTML = icon(collapsed ? 'PanelLeftOpen' : 'PanelLeftClose')
}
function openMain() { if (window.innerWidth <= MOBILE_WIDTH && !$('#app').classList.contains('collapsed')) toggleSidebar() }
function showConversation(id, navigate = true) {
  const item = data.find((entry) => entry.id === id)
  if (!item) return
  state.active = id
  $$('.utility').forEach((button) => button.removeAttribute('aria-current'))
  $('#view-title').textContent = item.title; $('#breadcrumb').textContent = item.project
  $('#header-state').innerHTML = statusMarkup(item, true)
  $('.main-scroll').classList.remove('manager-scroll'); $('#composer').hidden = false
  const message = id === 'nav' ? '首字耗时好像把初始化时间也算进去了，帮我检查一下。' : '继续处理：' + escapeHtml(item.title)
  const response = id === 'nav' ? '<h2>已定位到计时起点</h2><p>当前记录从会话初始化开始，包含了代理连接和上下文准备的耗时。我正在核对请求发出与首个有效输出之间的事件顺序。</p><ul><li>将请求时间与初始化时间分别记录。</li><li>首个有效内容到达后计算首字耗时。</li><li>检查重试和中断恢复时的统计结果。</li></ul>' : '<h2>' + escapeHtml(item.title) + '</h2><p>项目：' + item.project + '</p><p>任务状态：' + STATUS[item.status] + '</p>'
  $('#thread').innerHTML = '<p class="user-message">' + message + '</p><div class="agent-label"><img src="public/icon-32x32.png" alt="" />iyw-claw<small>工作空间</small></div>' + response + '<div class="activity-row">' + statusMarkup(item, true) + '<span>最近更新 · ' + (item.age ? item.age + ' 天前' : '今天') + '</span></div>'
  $('#thread').insertAdjacentHTML('beforeend', (messages.get(id) || []).map((text) => '<p class="user-message">' + escapeHtml(text) + '</p>').join(''))
  renderList()
  if (navigate) openMain()
}
function showSearchResults() {
  const query = $('#search-input').value.trim().toLocaleLowerCase()
  const matches = data.filter((item) => (item.title + item.project).toLocaleLowerCase().includes(query))
  $('#search-results').innerHTML = matches.map((item) => '<button class="search-result" data-conversation="' + item.id + '">' + icon('MessageSquare') + '<span class="result-copy"><strong>' + escapeHtml(item.title) + '</strong><small>' + item.project + '</small></span>' + statusMarkup(item, true) + '</button>').join('') || '<p class="empty-list">没有找到匹配的会话</p>'
  $('#search-count').textContent = matches.length + ' 个会话'
}
function openSearch() { closePopover(); $('#search-input').value = ''; showSearchResults(); $('#search-dialog').showModal(); $('#search-input').focus() }
function showDetail(key) {
  closePopover()
  const sections = { settings: 'appearance', usage: 'usage', connectors: 'connectors', memory: 'user-memory' }
  if (sections[key]) {
    const request = new CustomEvent('app://open-settings-dialog', { cancelable: true, detail: { section: sections[key], agentType: null } })
    if (window.dispatchEvent(request)) toast('独立预览未连接设置窗口')
    return
  }
  const details = {
    profile: ['账户资料', '<div class="detail-line"><span>昵称</span><strong>iyw</strong></div><div class="detail-line"><span>账户 ID</span><span>IYW-0826</span></div><div class="detail-line"><span>组织</span><span>未设置</span></div>'],
    refresh: ['积分余额', '<div class="detail-line"><span>当前余额</span><strong>1,240 积分</strong></div><p>当前离线，暂时无法刷新余额。</p>'],
    login: ['账户未登录', '<p>登录后查看账户资料和积分余额。</p><button class="secondary-button" id="return-account">返回预览账户</button>'],
    update: ['版本信息', '<div class="detail-line"><span>当前版本</span><strong>v0.1.237</strong></div><p>暂时无法连接更新服务。</p>']
  }
  const detail = details[key]
  $('#detail-title').textContent = detail[0]; $('#detail-body').innerHTML = detail[1]; $('#detail-dialog').showModal()
}
async function copyId() {
  try { await navigator.clipboard.writeText('IYW-0826'); toast('已复制 IYW-0826') }
  catch { toast('无法访问剪贴板，账户 ID：IYW-0826') }
}
function showUtility(key, source) {
  closePopover(); state.active = null; renderList()
  $('#composer').hidden = true; $('.main-scroll').classList.remove('manager-scroll')
  $$('.utility').forEach((button) => { if (button === source) button.setAttribute('aria-current', 'page'); else button.removeAttribute('aria-current') })
  if (key === 'sessions') { window.sessionManager.open(); openMain(); return }
  const labels = { automations: '自动化', skills: '技能中心', resources: '资源库' }
  const title = labels[key] || '工作台'
  $('#view-title').textContent = title; $('#breadcrumb').textContent = '工作空间'; $('#header-state').textContent = ''
  $('#thread').innerHTML = key === 'automations' ? '<h2>自动化</h2><div class="detail-line"><span>发布前回归检查</span><span class="status-failed">失败</span></div><div class="detail-line"><span>每日依赖检查</span><span class="status-failed">失败</span></div>' : key === 'skills' ? '<h2>技能中心</h2><div class="detail-line"><span>代码审查</span><span>已安装</span></div><div class="detail-line"><span>文档整理</span><span>已安装</span></div><div class="detail-line"><span>浏览器自动化</span><span>可用</span></div>' : '<h2>资源库</h2><div class="detail-line"><span>项目资料</span><span>12 个文件</span></div><div class="detail-line"><span>最近上传</span><span>今天</span></div>'
  openMain()
}
function handleDocumentClick(event) {
  const target = event.target
  if (state.popover && !state.popover.contains(target) && !state.anchor.contains(target)) closePopover()
  const sectionToggle = target.closest('[data-section-toggle]')
  if (sectionToggle) {
    const id = sectionToggle.dataset.sectionToggle, expanded = sectionToggle.getAttribute('aria-expanded') === 'true'
    if (expanded) collapsedSections.add(id); else collapsedSections.delete(id)
    sectionToggle.setAttribute('aria-expanded', String(!expanded)); $('#' + sectionToggle.getAttribute('aria-controls')).hidden = expanded
  }
  const conversation = target.closest('[data-conversation]')
  if (conversation) { $('#search-dialog').close(); showConversation(conversation.dataset.conversation) }
  const project = target.closest('[data-project]')
  if (project) { const id = project.dataset.project; if (state.closed.has(id)) state.closed.delete(id); else state.closed.add(id); renderList() }
  if (target.closest('#project-create-action')) {
    closePopover()
    $('#view-title').textContent = '新建项目'
    $('#breadcrumb').textContent = '工作空间'
    $('#header-state').textContent = ''
    $('#thread').innerHTML = '<div class="agent-label"><img src="public/icon-32x32.png" alt="" />iyw-claw</div><h2>创建一个新的项目</h2><p>为项目选择名称和工作目录，然后开始新的编码会话。</p><textarea class="task-input" aria-label="项目名称" placeholder="输入项目名称..."></textarea>'
    $('#composer').hidden = true
    return
  }
  if (target.closest('#project-folder-action')) {
    closePopover()
    toast('选择文件夹后将在此处打开项目')
    return
  }
  const recentProject = target.closest('[data-project-recent]')
  if (recentProject) {
    closePopover()
    $('#breadcrumb').textContent = recentProject.dataset.projectRecent
    toast('已切换到 ' + recentProject.dataset.projectRecent)
    return
  }
  const detail = target.closest('[data-detail]'); if (detail) showDetail(detail.dataset.detail)
  const theme = target.closest('[data-theme-choice]'); if (theme) setTheme(theme.dataset.themeChoice)
  const utility = target.closest('[data-view]'); if (utility) showUtility(utility.dataset.view, utility)
  if (target.closest('[data-close-dialog]')) target.closest('dialog').close()
  if (target.closest('[data-close-popover]')) closePopover(true)
  if (target.closest('#copy-id')) void copyId()
  if (target.closest('#return-account')) { state.signedIn = true; $('#dock-name').textContent = 'iyw'; $('#dock-plan').textContent = '1,240 积分'; $('#detail-dialog').close() }
}
hydrateIcons()
renderOptions()
showConversation('nav', false)
document.addEventListener('click', handleDocumentClick)
$('#toggle').onclick = toggleSidebar
$('#rail-chat').onclick = toggleSidebar
$('#search-trigger').onclick = openSearch
$('#search-input').oninput = showSearchResults
$('#filter-trigger').onclick = (event) => { event.stopPropagation(); togglePopover('filter-panel', event.currentTarget) }
$('#new-project').onclick = (event) => { event.stopPropagation(); togglePopover('project-panel', event.currentTarget) }
$('#open-folder').onclick = (event) => { event.stopPropagation(); togglePopover('project-panel', event.currentTarget) }
$('#workbench-toggle').onclick = () => {
  const expanded = $('#workbench-toggle').getAttribute('aria-expanded') === 'true'
  $('#workbench-toggle').setAttribute('aria-expanded', String(!expanded))
  $('#workbench-nav').hidden = expanded
}
$('#account-trigger').onclick = (event) => {
  event.stopPropagation()
  if (!state.signedIn) { showDetail('login'); return }
  togglePopover('account-panel', event.currentTarget)
}
$('#filter-panel').onchange = (event) => { const { name, value } = event.target; if (['status', 'time'].includes(name)) { state[name] = value; state.closed.clear(); renderList() } }
$('#clear-filter').onclick = resetFilter
$('#reset-filter').onclick = resetFilter
$('#logout-trigger').onclick = () => { closePopover(); $('#logout-dialog').showModal() }
$('#confirm-logout').onclick = () => { state.signedIn = false; $('#dock-name').textContent = '未登录'; $('#dock-plan').textContent = '登录账户'; $('#logout-dialog').close(); $('#account-trigger').focus(); toast('已退出当前预览账户') }
$('#new-chat').onclick = () => {
  closePopover(); state.active = null; renderList(); $$('.utility').forEach((button) => button.removeAttribute('aria-current'))
  $('#view-title').textContent = '新建会话'; $('#breadcrumb').textContent = '个人空间'; $('#header-state').textContent = ''
  $('.main-scroll').classList.remove('manager-scroll'); $('#composer').hidden = false
  $('#thread').innerHTML = '<div class="agent-label"><img src="public/icon-32x32.png" alt="" />iyw-claw</div><h2>今天想完成什么？</h2>'
  openMain(); $('#message-input').focus()
}
$('#composer').onsubmit = (event) => {
  event.preventDefault()
  const input = $('#message-input')
  if (!input.value.trim()) { input.focus(); return }
  if (!state.active) { const id = 'local-' + Date.now(); data.unshift({ id, title: input.value.trim(), status: 'planning', age: 0, group: 'recent', project: '个人空间' }); state.active = id }
  messages.set(state.active, [...(messages.get(state.active) || []), input.value.trim()])
  showConversation(state.active); $('#draft-state').textContent = '已保存'
  input.value = ''
  $('.main-scroll').scrollTop = $('.main-scroll').scrollHeight
}
$('#message-input').oninput = (event) => { $('#draft-state').textContent = event.target.value.trim() ? '草稿' : '' }
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') closePopover(true)
  if (event.key === 'Tab' && state.popover) {
    const controls = $$('button:not(:disabled), input:checked', state.popover)
    const first = controls[0], last = controls.at(-1)
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
    if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
  }
})
$$('dialog').forEach((dialog) => dialog.addEventListener('click', (event) => { const bounds = dialog.getBoundingClientRect(); if (event.target === dialog && (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom)) dialog.close() }))
window.addEventListener('resize', positionPopover)
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => { if (state.theme === 'system') setTheme('system') })
