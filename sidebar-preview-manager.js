const managerState = { query: '', status: 'all', project: 'all', selected: new Set(), pendingDelete: [] }
const agentNames = ['Codex', 'Claude Code', 'Gemini CLI']
data.forEach((item, index) => { item.agent = agentNames[index % agentNames.length] })
renderList()
function managerEntries() {
  const query = managerState.query.trim().toLocaleLowerCase()
  return data.filter((item) => (item.title + item.project + (item.agent || 'Codex')).toLocaleLowerCase().includes(query) && (managerState.status === 'all' || item.status === managerState.status) && (managerState.project === 'all' || item.project === managerState.project))
}
function managerRow(item) {
  const checked = managerState.selected.has(item.id)
  const pinned = item.group === 'pinned'
  const date = item.age === 0 ? '今天' : item.age === 1 ? '昨天' : item.age + ' 天前'
  return '<tr' + (checked ? ' class="selected-row"' : '') + '><td><input type="checkbox" data-select-session="' + item.id + '" aria-label="选择 ' + escapeHtml(item.title) + '"' + (checked ? ' checked' : '') + ' /></td><td><button class="session-title" data-conversation="' + item.id + '">' + escapeHtml(item.title) + '</button></td><td><span class="project-text">' + item.project + '</span></td><td><span class="agent-name">' + (item.agent || 'Codex') + '</span></td><td>' + statusMarkup(item, true) + '</td><td>' + date + '</td><td><button class="icon-button pin-button' + (pinned ? ' pinned' : '') + '" data-pin-session="' + item.id + '" title="' + (pinned ? '取消置顶' : '置顶') + '" aria-label="' + (pinned ? '取消置顶' : '置顶') + ' ' + escapeHtml(item.title) + '" aria-pressed="' + pinned + '">' + icon('Pin') + '</button></td></tr>'
}
function renderManagerRows() {
  if (!$('#session-rows')) return
  const entries = managerEntries()
  $('#session-rows').innerHTML = entries.map(managerRow).join('') || '<tr><td colspan="7"><div class="manager-empty">' + icon('Search') + '<strong>没有匹配的会话</strong><button class="secondary-button" data-manager-reset>清除筛选</button></div></td></tr>'
  $('#manager-count').textContent = data.length + ' 个会话'
  $('#manager-footer').textContent = '显示 ' + entries.length + ' 个会话 · 共 ' + data.length + ' 个'
  $('#selected-count').textContent = managerState.selected.size ? '已选 ' + managerState.selected.size + ' 项' : '未选择'
  const all = $('#select-all')
  const selectedVisible = entries.filter((item) => managerState.selected.has(item.id)).length
  all.checked = entries.length > 0 && selectedVisible === entries.length
  all.indeterminate = selectedVisible > 0 && selectedVisible < entries.length
  all.disabled = entries.length === 0
  const busy = data.some((item) => managerState.selected.has(item.id) && item.status === 'running')
  $$('[data-batch-action]').forEach((button) => { button.disabled = !managerState.selected.size || (busy && button.dataset.batchAction !== 'pin'); button.title = busy && button.dataset.batchAction !== 'pin' ? '进行中的会话需先停止' : button.textContent.trim() })
}
function openManager() {
  $('#view-title').textContent = '会话管理器'; $('#breadcrumb').textContent = '工作空间'; $('#header-state').textContent = ''
  $('.main-scroll').classList.add('manager-scroll'); $('#composer').hidden = true
  managerState.selected.clear()
  const projects = [...new Set(data.map((item) => item.project))]
  $('#thread').innerHTML = '<section class="session-manager"><header class="manager-heading"><div><h2>会话管理器</h2><span id="manager-count"></span></div><button class="manager-new" data-manager-new>' + icon('SquarePen') + '新建会话</button></header><div class="manager-controls"><label class="manager-search">' + icon('Search') + '<input id="manager-query" type="search" aria-label="在会话管理器中搜索" placeholder="搜索会话、项目或代理" /></label><select id="manager-project" aria-label="按项目筛选"><option value="all">全部项目</option>' + projects.map((project) => '<option value="' + project + '">' + project + '</option>').join('') + '</select><select id="manager-status" aria-label="按状态筛选">' + Object.entries(STATUS).map(([value, label]) => '<option value="' + value + '">' + label + '</option>').join('') + '</select></div><div class="batch-toolbar"><span id="selected-count"></span><button data-batch-action="pin">' + icon('Pin') + '置顶</button><button data-batch-action="complete">' + icon('Check') + '标记完成</button><button class="delete-action" data-batch-action="delete">' + icon('Trash2') + '删除</button></div><div class="session-table-scroll" role="region" aria-label="会话列表" tabindex="0"><table class="session-table"><thead><tr><th><input type="checkbox" id="select-all" aria-label="选择当前结果中的全部会话" /></th><th>会话名称</th><th>项目</th><th>代理</th><th>状态</th><th>更新</th><th><span class="sr-only">置顶操作</span></th></tr></thead><tbody id="session-rows"></tbody></table></div><footer id="manager-footer"></footer></section>'
  $('#manager-query').value = managerState.query; $('#manager-project').value = managerState.project; $('#manager-status').value = managerState.status
  renderManagerRows()
}
function togglePin(item, pinned = item.group !== 'pinned') {
  item.group = pinned ? 'pinned' : item.project === 'iyw-claw' ? 'claw' : item.project === 'iyw-fusion-api' ? 'fusion' : 'recent'
}
function batchAction(action) {
  const items = data.filter((item) => managerState.selected.has(item.id))
  if (!items.length) return
  if (action !== 'pin' && items.some((item) => item.status === 'running')) return
  if (action === 'delete') {
    managerState.pendingDelete = items.map((item) => item.id)
    $('#detail-title').textContent = '删除 ' + items.length + ' 个会话？'
    $('#detail-body').innerHTML = '<p>所选会话将从当前列表中移除，此操作无法撤销。</p><div class="dialog-actions"><button class="secondary-button" data-close-dialog>取消</button><button class="danger-button" id="confirm-session-delete">删除会话</button></div>'
    $('#detail-dialog').showModal(); return
  }
  items.forEach((item) => { if (action === 'pin') togglePin(item, true); else item.status = 'completed' })
  managerState.selected.clear(); renderList(); renderManagerRows(); toast(action === 'pin' ? '已置顶所选会话' : '已标记为完成')
}
function deleteSelectedSessions() {
  const removing = new Set(managerState.pendingDelete)
  for (let index = data.length - 1; index >= 0; index--) { if (removing.has(data[index].id)) { messages.delete(data[index].id); data.splice(index, 1) } }
  managerState.pendingDelete = []; managerState.selected.clear()
  $('#detail-dialog').close(); renderList(); renderManagerRows(); toast('所选会话已删除')
}
function handleManagerChange(event) {
  const target = event.target
  if (target.matches('[data-select-session]')) { const id = target.dataset.selectSession; if (target.checked) managerState.selected.add(id); else managerState.selected.delete(id) }
  else if (target.id === 'select-all') { managerEntries().forEach((item) => { if (target.checked) managerState.selected.add(item.id); else managerState.selected.delete(item.id) }) }
  else if (target.id === 'manager-project') { managerState.project = target.value; managerState.selected.clear() }
  else if (target.id === 'manager-status') { managerState.status = target.value; managerState.selected.clear() }
  else return
  renderManagerRows()
}
function handleManagerClick(event) {
  const target = event.target
  const pin = target.closest('[data-pin-session]')
  if (pin) { const item = data.find((entry) => entry.id === pin.dataset.pinSession); if (item) togglePin(item); renderList(); renderManagerRows() }
  const batch = target.closest('[data-batch-action]'); if (batch) batchAction(batch.dataset.batchAction)
  if (target.closest('#confirm-session-delete')) deleteSelectedSessions()
  if (target.closest('[data-manager-new]')) $('#new-chat').click()
  if (target.closest('[data-manager-reset]')) { managerState.query = ''; managerState.project = 'all'; managerState.status = 'all'; openManager() }
}
document.addEventListener('change', handleManagerChange)
document.addEventListener('click', handleManagerClick)
document.addEventListener('input', (event) => { if (event.target.id === 'manager-query') { managerState.query = event.target.value; managerState.selected.clear(); renderManagerRows() } })
window.sessionManager = { open: openManager }
