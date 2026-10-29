/* Tools inventory owns only #tools-region; account/session/header remain separate. */
(() => {
  const root = document.getElementById('tools-region');
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  let authenticated = false;
  let loading = false;
  let inventoryPending = Promise.resolve();
  let connecting = false;
  let disconnecting = false;
  let pendingDisconnect = null;
  let pendingReconnect = null;
  let generation = 0;
  let entries = [];
  let selectedTool = null, filter = 'all', search = '';
  const detailRoot = document.getElementById('tool-detail-region');
  root.innerHTML = '<div class="tools-controls"><div class="tools-filters" role="group" aria-label="按连接状态筛选"><button data-tool-filter="all" class="selected">全部</button><button data-tool-filter="connected">已接入</button><button data-tool-filter="available">待接入</button></div><input type="search" class="tools-search" placeholder="搜索工具或模型" aria-label="搜索工具或模型"></div><div class="tools-feedback" role="status" aria-live="polite"></div><div class="tools-list"></div>';
  const searchInput = root.querySelector('.tools-search');
  searchInput.oninput = () => { search = searchInput.value.toLowerCase().trim(); render(); };
  root.querySelectorAll('[data-tool-filter]').forEach(button => { button.onclick = () => { filter = button.dataset.toolFilter; render(); }; });
  const message = root.querySelector('.tools-feedback');
  const refreshButton = document.getElementById('detect-tools');
  const list = root.querySelector('.tools-list');
  function feedback(text, error = false) { message.textContent = text; message.classList.toggle('error', error); const detailMessage = detailRoot.querySelector('.tool-operation-status'); if (detailMessage) { detailMessage.textContent = text; detailMessage.classList.toggle('error', error); } }
  const resultDialog = document.createElement('dialog');
  resultDialog.className = 'tool-result-dialog';
  resultDialog.innerHTML = '<h3></h3><p class="tool-result-detail" role="status"></p><div class="tool-actions"><button class="primary result-close" type="button">知道了</button></div>';
  document.body.append(resultDialog);
  resultDialog.querySelector('.result-close').onclick = () => resultDialog.close();
  function showResult(title, detail) {
    resultDialog.querySelector('h3').textContent = title;
    resultDialog.querySelector('.tool-result-detail').textContent = detail;
    if (!resultDialog.open) resultDialog.showModal();
  }
  function text(tag, value, className) {
    const element = document.createElement(tag);
    element.textContent = value;
    if (className) element.className = className;
    return element;
  }
  function connectionLabel(entry) {
    if (!entry.connection) return entry.installed ? '已安装，尚未接入' : '未安装';
    return ({legacy:'旧版连接，归属未知', configuration_changed:'配置已改变或不可用', configured_untested:'已配置，未测试', test_passed:'文本输出测试通过', test_failed:'文本输出测试失败'})[entry.connection.state] || '已配置，未测试';
  }
  function stateBadge(entry) { return text('span', connectionLabel(entry), `tool-state ${entry.connection?.state || 'unconnected'}`); }
  function toolIcon(entry) {
    const icon = text('div', '', 'tool-icon'); const image = document.createElement('img');
    image.src = `assets/tools/${entry.id}.${({'workbuddy':'svg',openclaw:'svg',codex:'ico','deepseek-harness':'ico'})[entry.id] || 'png'}`;
    image.alt = ''; icon.append(image); return icon;
  }
  function actionButton(label, className, action, disabled = false) {
    const button = text('button', label, className); button.type = 'button'; button.disabled = disabled; button.onclick = action; return button;
  }
  async function launch(entry) {
    const current = generation;
    try { await invoke('launch_connected_tool', {tool:entry.id}); if (authenticated && current === generation) feedback('已请求启动。现有会话请退出后重新启动以读取配置。'); }
    catch (error) { if (authenticated && current === generation) showResult('无法打开工具', String(error)); }
  }
  async function connect(entry) {
    const current = generation;
    try {
      if (entry.connection && entry.connection.state !== 'legacy') { await beginReconnect(entry); return; }
      await invoke('prepare_tool_connect', {tool:entry.id});
      if (!authenticated || current !== generation) return;
      const event = new CustomEvent('tokenmarket-connect-request', {detail:{tool:entry.id}, cancelable:true});
      if (window.dispatchEvent(event)) feedback('接入功能正在准备中。');
    } catch (error) { if (authenticated && current === generation) { feedback(String(error), true); showResult('接入未完成', String(error)); await refresh(); } }
  }
  function actions(entry, detailed = false) {
    const testing = Boolean(window.TokenMarketConnectionTests?.busy);
    const locked = loading || connecting || disconnecting || testing;
    const group = text('div', '', 'tool-actions');
    if (entry.connection && entry.connection.state !== 'legacy') {
      group.append(actionButton('打开工具', 'primary tool-launch', () => launch(entry), !entry.installed || locked));
      if (detailed) {
        const test = actionButton('测试连接', 'tool-test', () => window.dispatchEvent(new CustomEvent('tokenmarket-test-request', {detail:{id:entry.id,name:entry.name,connection:entry.connection}})), locked || entry.connection.state === 'configuration_changed');
        if (entry.connection.state === 'configuration_changed') test.title = '请先检查配置或重新接入';
        group.append(test);
        const menu = text('details', '', 'tool-menu'); menu.append(text('summary', '管理连接'));
        const options = text('div', '', 'tool-menu-options');
        options.append(actionButton('重新接入', 'tool-connect', () => { menu.open = false; void connect(entry); }, !entry.canConnect || locked));
        options.append(actionButton('取消接入', 'danger tool-disconnect', () => { menu.open = false; confirmDisconnectStart(entry); }, locked));
        menu.append(options); group.append(menu);
      }
    } else if (!entry.connection && entry.installed) {
      group.append(actionButton('接入', 'primary tool-connect', () => connect(entry), !entry.canConnect || locked));
    } else if (!entry.installed) {
      group.append(actionButton('前往安装', 'tool-download', async () => {
        try { await invoke('download_tool', {tool:entry.id}); feedback('已打开官网。安装完成后点击“重新检测”。'); }
        catch (error) { showResult('无法打开安装页面', String(error)); }
      }, locked));
    }
    return group;
  }
  function detailRow(label, value) { const row = text('div', '', 'info-row'); row.append(text('span', label), text('strong', value)); return row; }
  function showDetail(entry) {
    selectedTool = entry.id; renderDetail(entry); window.TokenMarketShell.show('detail', entry.name);
  }
  function renderDetail(entry) {
    detailRoot.replaceChildren();
    const connection = entry.connection, legacy = connection?.state === 'legacy';
    const header = text('header', '', 'page-heading tool-detail-heading');
    const title = text('div', '', 'tool-detail-title'); const names = text('div'); names.append(text('h1', entry.name), text('p', entry.installed ? '已检测到本机安装' : '尚未安装')); title.append(toolIcon(entry), names);
    const back = actionButton('返回工具列表', 'quiet', () => window.TokenMarketShell.show('tools')); header.append(title, back); detailRoot.append(header); const operation = text('div', message.textContent, `tools-feedback tool-operation-status${message.classList.contains('error') ? ' error' : ''}`); operation.setAttribute('role', 'status'); detailRoot.append(operation);
    if (connection) {
      const notice = text('div', '', `notice ${legacy || connection.state === 'configuration_changed' ? 'warning' : connection.state === 'test_failed' ? 'error' : connection.state === 'test_passed' ? 'success' : ''}`);
      notice.append(text('h3', connectionLabel(entry)));
      const note = legacy ? '检测到旧版连接记录，所属账号与计费空间未知。请在原工具与平台中核对配置。' : connection.state === 'configuration_changed' ? '当前配置与接入记录不一致，测试已停用。请检查配置或重新接入。' : connection.state === 'test_passed' ? '仅验证一次完整文本输出；流式、工具调用与 Agent 运行表现请在工具中确认。' : connection.state === 'test_failed' ? '未通过文本输出测试。请检查权限、余额与模型可用性后重试。' : '配置已保存，尚未发起模型测试。可以先打开工具；需要时再确认费用并测试。';
      notice.append(text('p', note)); detailRoot.append(notice);
    } else detailRoot.append(text('p', entry.installed ? '工具已安装，接入后即可使用 Token Market 模型。' : '先安装工具，再返回重新检测和接入。', 'notice'));
    detailRoot.append(actions(entry, true));
    const grid = text('div', '', 'tool-detail-grid'), left = text('div');
    const models = text('section', '', 'sheet'); models.append(text('h2', '已配置模型'));
    if (connection?.models?.length) {
      connection.models.forEach(model => { const item = text('div', '', 'tool-model-entry'); item.append(text('code', model), text('span', '已配置', 'tool-model-tag')); models.append(item); });
    } else models.append(text('p', '接入后会显示该连接的可用模型。'));
    left.append(models);
    if (connection?.testResult) {
      const result = text('section', '', 'sheet'); result.append(text('h2', '最近一次文本测试'), detailRow('测试模型', connection.testResult.model), detailRow('测试时间', connection.testResult.testedAt), text('p', connection.testResult.detail)); left.append(result);
    }
    const guidance = text('section', '', 'sheet'); guidance.append(text('h2', '使用前提醒'), text('p', '配置完成后，请完全退出并重新打开已有工具会话。'));
    if (entry.id === 'openclaw') guidance.append(text('p', 'OpenClaw 需要重启 Gateway 或会话，已有模型白名单仍可能限制可选模型。'));
    if (entry.id === 'opencode') guidance.append(text('p', '项目或运行时配置可能覆盖全局模型设置。'));
    left.append(guidance);
    const context = text('section', '', 'sheet'); context.append(text('h2', '连接归属'), detailRow('所属账号', legacy ? '未知（旧版记录）' : connection?.owner?.name || '尚未接入'), detailRow('计费空间', legacy ? '未知（旧版记录）' : connection?.boundWorkspace?.name || '尚未接入'), detailRow('安装状态', entry.installed ? '已安装' : '未安装'));
    context.append(text('p', '已有连接保留自己的计费空间。需要更换归属时，请确认重新接入。'));
    grid.append(left, context); detailRoot.append(grid);
  }
  function render() {
    list.replaceChildren();
    const visible = entries.filter(entry => (filter === 'all' || filter === 'connected' && Boolean(entry.connection) || filter === 'available' && entry.installed && !entry.connection) && `${entry.name} ${(entry.connection?.models || []).join(' ')}`.toLowerCase().includes(search));
    root.querySelectorAll('[data-tool-filter]').forEach(button => {
      const kind = button.dataset.toolFilter;
      const count = entries.filter(entry => kind === 'all' || kind === 'connected' && Boolean(entry.connection) || kind === 'available' && entry.installed && !entry.connection).length;
      button.textContent = `${({all:'全部',connected:'已接入',available:'待接入'})[kind]} ${count}`;
      button.classList.toggle('selected', filter === kind); button.setAttribute('aria-pressed', String(filter === kind));
    });
    if (!entries.length) {
      const empty = text('div', '', 'tools-empty');
      empty.append(text('h2', loading ? '正在检测本机工具…' : message.classList.contains('error') ? '暂时无法检测本机工具' : '没有检测到工具'), text('p', loading ? '请稍候，安装状态即将更新。' : message.classList.contains('error') ? message.textContent : '安装工具后，点击重新检测。'));
      if (!loading) empty.append(actionButton('重新检测', '', () => refreshButton.click()));
      list.append(empty);
    } else if (!visible.length) {
      const empty = text('div', '', 'tools-empty'); empty.append(text('h2', '没有匹配的工具'), text('p', '试试其他名称，或查看全部工具。'), actionButton('查看全部工具', '', () => { search = ''; filter = 'all'; searchInput.value = ''; render(); })); list.append(empty);
    }
    visible.forEach(entry => {
      const row = text('article', '', 'tool-row'); row.dataset.tool = entry.id;
      const top = text('div', '', 'tool-card-top'); const title = text('div', '', 'tool-title'); title.append(text('h3', entry.name), stateBadge(entry)); top.append(toolIcon(entry), title);
      const content = text('div', '', 'tool-details');
      if (entry.connection) {
        content.append(text('p', entry.connection.state === 'legacy' ? '计费空间未知（旧版记录）' : `计费空间  ${entry.connection.boundWorkspace.name}`, 'tool-description'));
        const count = entry.connection.models.length; content.append(text('p', count === 1 ? entry.connection.models[0] : `${count} 个已配置模型`, 'tool-model-summary'));
      } else content.append(text('p', entry.installed ? '接入时将使用当前空间' : '安装后返回此处重新检测', 'tool-description'));
      const footer = text('div', '', 'tool-card-footer'); footer.append(actionButton(entry.connection ? '连接详情' : '了解工具', 'quiet tool-detail', () => showDetail(entry)), actions(entry));
      row.append(top, content, footer); list.append(row);
    });
    const selected = entries.find(entry => entry.id === selectedTool);
    if (selected) renderDetail(selected);
  }
  async function refresh(waitForCurrent = false) {
    if (!authenticated) return;
    if (loading) { if (waitForCurrent === true) { await inventoryPending; return refresh(true); } return; }
    let finishInventory;
    inventoryPending = new Promise(resolve => { finishInventory = resolve; });
    loading = true; const current = generation;
    refreshButton.disabled = true; refreshButton.textContent = '检测中…';
    render();
    let success = false;
    try {
      const next = await invoke('desktop_tool_inventory');
      if (authenticated && current === generation) { entries = next; success = true; feedback('安装状态已更新'); }
    } catch (error) { if (authenticated && current === generation) { entries = []; feedback(String(error), true); } }
    finally {
      loading = false; finishInventory(); refreshButton.disabled = false; refreshButton.textContent = '重新检测';
      if (authenticated && current === generation) render();
      else if (authenticated) refresh();
    }
    return success;
  }
  const disconnectConfirm = document.createElement('dialog');
  disconnectConfirm.className = 'disconnect-confirm-dialog';
  document.body.append(disconnectConfirm);
  function confirmDisconnectStart(entry) {
    disconnectConfirm.replaceChildren(text('h3', '确认取消接入'), text('p', `将恢复 ${entry.name} 本次连接管理的配置，并尝试停用对应 Key。计费空间：${entry.connection.boundWorkspace.name}。`));
    const buttons = text('div', '', 'tool-actions');
    buttons.append(actionButton('保留连接', '', () => disconnectConfirm.close()), actionButton('确认取消接入', 'danger', () => { disconnectConfirm.close(); runDisconnect(entry, {revision:null,resolutions:[]}); }));
    disconnectConfirm.append(buttons); disconnectConfirm.showModal();
  }
  const disconnectDialog = document.createElement('dialog'); disconnectDialog.className = 'disconnect-dialog';
  disconnectDialog.innerHTML = '<h3>配置与恢复内容发生冲突</h3><p>检测到接入后的修改。请选择保留当前内容，或恢复接入前的原值。为保护凭据，此处只显示文件与配置项编号；取消将保留连接和所有文件。</p><div class="disconnect-conflicts"></div><div class="tool-actions"><button class="disconnect-cancel">暂不取消接入</button><button class="primary disconnect-confirm" disabled>确认恢复并取消接入</button></div>';
  const confirmDisconnect = disconnectDialog.querySelector('.disconnect-confirm');
  disconnectDialog.querySelector('.disconnect-cancel').onclick = () => { pendingDisconnect = null; disconnectDialog.close(); };
  disconnectDialog.addEventListener('cancel', () => { pendingDisconnect = null; });
  confirmDisconnect.onclick = () => {
    if (!pendingDisconnect) return;
    const {entry,outcome} = pendingDisconnect;
    const choices = [...disconnectDialog.querySelectorAll('select')];
    if (choices.some(choice => !choice.value)) return;
    const resolutions = outcome.conflicts.map((conflict,index) => ({...conflict,choice:choices[index].value}));
    disconnectDialog.close(); pendingDisconnect = null;
    runDisconnect(entry, {revision:outcome.revision,resolutions});
  };
  async function runDisconnect(entry, request) {
    if (!authenticated || connecting || disconnecting) return;
    const current = generation; disconnecting = true; render(); feedback(`正在安全恢复 ${entry.name} 的本地配置…`);
    try {
      const outcome = await invoke('disconnect_tool', {tool:entry.id,expectedKeyId:entry.connection.keyId,request});
      if (!authenticated || current !== generation) return;
      if (outcome.state === 'conflicts') {
        pendingDisconnect = {entry,outcome};
        const choices = disconnectDialog.querySelector('.disconnect-conflicts'); choices.replaceChildren();
        confirmDisconnect.disabled = true;
        outcome.conflicts.forEach(conflict => {
          const label = text('label', `文件 ${conflict.file + 1} · 配置项 ${conflict.field + 1} `);
          const select = document.createElement('select');
          [['','请选择'],['keep_current','保留当前内容'],['restore_original','恢复接入前原值']].forEach(([value,name]) => { const option = text('option',name); option.value = value; select.append(option); });
          select.onchange = () => { confirmDisconnect.disabled = [...choices.querySelectorAll('select')].some(choice => !choice.value); };
          label.append(select); choices.append(label);
        });
        disconnectDialog.showModal(); feedback('请明确选择每个冲突的处理方式。');
      } else {
        await refresh(true);
        if (!authenticated || current !== generation) return;
        const detail = outcome.remoteDisabled ? `${entry.name} 已在本地取消接入，原 Key 已停用。` : `${entry.name} 已在本地取消接入；原 Key 未确认停用，请在平台 Key 管理中检查。`;
        feedback(detail); showResult('取消接入完成', detail);
      }
    } catch (error) { if (authenticated && current === generation) { feedback(String(error), true); showResult('取消接入未完成', String(error)); } }
    finally { disconnecting = false; if (authenticated && current === generation) render(); }
  }
  const reconnectDialog = document.createElement('dialog'); reconnectDialog.className = 'reconnect-dialog';
  reconnectDialog.innerHTML = '<h3>确认重新接入</h3><p class="reconnect-from"></p><p class="reconnect-to"></p><p>将使用当前登录账号与当前空间取得新 Key 并重新读取模型目录。新配置保存后才尝试停用旧 Key；跨账号可能无权停用旧 Key。</p><div class="tool-actions"><button class="reconnect-cancel">取消</button><button class="primary reconnect-confirm">确认重新接入</button></div>';
  document.body.append(reconnectDialog);
  reconnectDialog.querySelector('.reconnect-cancel').onclick = () => { pendingReconnect = null; reconnectDialog.close(); };
  reconnectDialog.addEventListener('cancel', () => { pendingReconnect = null; });
  reconnectDialog.querySelector('.reconnect-confirm').onclick = () => {
    const pending = pendingReconnect; pendingReconnect = null; reconnectDialog.close();
    if (pending) runConnect(pending.entry.id, pending);
  };
  async function beginReconnect(entry) {
    if (connecting || disconnecting || !authenticated || window.TokenMarketConnectionTests?.busy) return;
    const current = generation;
    const preview = await invoke('prepare_tool_reconnect', {tool:entry.id,expectedKeyId:entry.connection.keyId});
    if (!authenticated || current !== generation) return;
    pendingReconnect = {entry,preview};
    reconnectDialog.querySelector('.reconnect-from').textContent = `原连接：${preview.oldOwner.name} · ${preview.oldWorkspace.name}`;
    reconnectDialog.querySelector('.reconnect-to').textContent = `新连接：${preview.currentOwner.name} · ${preview.currentWorkspace.name}`;
    if (!reconnectDialog.open) reconnectDialog.showModal();
  }
  window.addEventListener('current-workspace-changed', () => { pendingReconnect = null; if (reconnectDialog.open) reconnectDialog.close(); });
  const connectDialog = document.createElement('dialog');
  connectDialog.className = 'connect-confirm-dialog';
  connectDialog.innerHTML = '<h3>确认接入</h3><p>将为此工具配置当前空间可用的模型。接入本身不会发起模型测试。</p><p class="connect-target"></p><div class="tool-actions"><button class="connect-cancel">取消</button><button class="primary connect-confirm">确认接入</button></div>';
  document.body.append(connectDialog);
  let pendingConnect = null;
  async function confirmInitialConnect(tool) {
    const current = generation;
    await window.TokenMarketWorkspace.refresh();
    if (!authenticated || current !== generation) return;
    const context = window.TokenMarketWorkspace.current;
    if (!context) { showResult('无法确认接入空间', '请先刷新账号与空间后重试。'); return; }
    connectDialog.querySelector('h3').textContent = `接入 ${supportedConnections[tool]}`;
    pendingConnect = {tool,ownerId:context.user.id,workspaceId:context.current_workspace.id};
    connectDialog.querySelector('.connect-target').textContent = `账号：${context.user.display_name || context.user.user_name} · 计费空间：${context.current_workspace.name}`;
    connectDialog.showModal();
  }
  connectDialog.querySelector('.connect-cancel').onclick = () => { pendingConnect = null; connectDialog.close(); };
  connectDialog.addEventListener('cancel', () => { pendingConnect = null; });
  connectDialog.querySelector('.connect-confirm').onclick = () => {
    const pending = pendingConnect; pendingConnect = null; connectDialog.close();
    const context = window.TokenMarketWorkspace.current;
    if (!pending || !context) return;
    if (context.user.id !== pending.ownerId || context.current_workspace.id !== pending.workspaceId) { showResult('账号或空间已改变', '请重新确认接入归属。'); return; }
    runConnect(pending.tool);
  };
  window.addEventListener('current-workspace-changed', () => { pendingConnect = null; if (connectDialog.open) connectDialog.close(); });
  const supportedConnections = {'claude-code':'Claude Code', codex:'Codex CLI', 'deepseek-harness':'DeepSeek Harness',workbuddy:'WorkBuddy',openclaw:'OpenClaw',opencode:'OpenCode'};
  let launchTool = 'claude-code';
  const dialog = document.createElement('dialog');
  dialog.innerHTML = '<h3></h3><p class="restart-notice"></p><p>模型测试由你另行发起，可能产生少量费用。</p><div class="tool-actions"><button class="primary launch">打开终端运行 Claude Code</button><button class="close">完成</button></div><p class="launch-error" role="status"></p>';
  document.body.append(dialog);
  document.body.append(disconnectDialog);
  dialog.querySelector('.close').onclick = () => dialog.close();
  dialog.querySelector('.launch').onclick = async () => {
    try { await invoke('launch_connected_tool', {tool:launchTool}); dialog.close(); }
    catch(error) { dialog.querySelector('.launch-error').textContent = String(error); }
  };
  window.addEventListener('tokenmarket-connect-request', event => {
    const tool = event.detail.tool;
    if (!supportedConnections[tool]) return;
    event.preventDefault(); confirmInitialConnect(tool).catch(error => { if (authenticated) showResult('无法确认接入空间', String(error)); });
  });
  async function runConnect(tool, replacement = null) {
    const name = supportedConnections[tool];
    if (!name || !authenticated || connecting || disconnecting || window.TokenMarketConnectionTests?.busy) return;
    if (!replacement && entries.find(entry => entry.id === tool)?.connection) {
      feedback('连接已改变，请刷新后使用重新接入。', true); return;
    }
    const actionGeneration = generation;
    connecting = true; render(); feedback(`正在接入 ${name}，请稍候…`);
    try {
      const result = replacement
        ? await invoke('reconnect_tool', {tool,expectedKeyId:replacement.entry.connection.keyId,expectedOwnerId:replacement.preview.currentOwner.id,expectedWorkspaceId:replacement.preview.currentWorkspace.id})
        : await invoke('connect_tool', {tool});
      const connected = replacement ? result.connection : result;
      const cleanupNotice = replacement ? (result.oldKeyDisabled ? ' 原 Key 已停用。' : ' 旧 Key 未停用，请在平台 Key 管理中检查；新连接已保存。') : '';
      if (!authenticated || actionGeneration !== generation) return;
      await refresh(true);
      if (!authenticated || actionGeneration !== generation) return;
      const modelNotice = !['claude-code','codex'].includes(tool) ? `已导入 ${connected.models.length} 个兼容模型，尚未测试。` : `已配置 ${connected.models[0]}，尚未测试。`;
      feedback(`${name} ${modelNotice}${cleanupNotice}`);
      launchTool = tool;
      dialog.querySelector('h3').textContent = `${name} 已接入`;
      const guidance = {
        'deepseek-harness':'Harness 当前版本可在后续请求重新读取配置；若现有会话仍使用旧设置，请退出后重新打开所选 profile。',
        workbuddy:'请完全退出并重新打开 WorkBuddy，在模型列表中选择导入模型。',
        openclaw:'请重启 OpenClaw Gateway 并重新打开会话。原有模型白名单或 agent 专属配置仍可能限制可选模型，请在 OpenClaw 中检查。',
        opencode:'请退出并重新启动 OpenCode，在模型列表中选择导入模型。项目或运行时的配置覆盖仍按 OpenCode 自身规则生效。',
      };
      dialog.querySelector('.restart-notice').textContent = `${modelNotice} ${guidance[tool] || `现有 ${name} 会话需要退出后重新启动，新终端将读取接入配置。`}`;
      dialog.querySelector('.launch').textContent = ['deepseek-harness','workbuddy'].includes(tool) ? `打开 ${name}` : `打开工具 / 终端运行 ${name}`;
      dialog.querySelector('.launch-error').textContent = cleanupNotice; dialog.showModal();
    } catch(error) { if (authenticated && actionGeneration === generation) { feedback(String(error), true); showResult('接入未完成', String(error)); } }
    finally { connecting = false; if (authenticated && actionGeneration === generation) render(); }
  }
  window.addEventListener('tokenmarket-test-busy', render);
  refreshButton.onclick = async () => {
    const current = generation;
    const success = await refresh(true);
    if (!authenticated || current !== generation) return;
    const detail = success ? entries.map(entry => `${entry.name}：${entry.installed ? '已安装' : '未安装'}${entry.connection ? '，已接入' : ''}`).join('\n') : message.textContent;
    showResult(success ? '检测完成' : '检测未完成', detail || '未检测到工具。');
  };
  window.addEventListener('focus', refresh);
  document.addEventListener('visibilitychange', () => { if (!document.hidden) refresh(); });
  setInterval(() => { if (!document.hidden) refresh(); }, 15000);
  window.TokenMarketTools = {
    refresh,
    setAuthenticated(value) {
      value = Boolean(value);
      root.hidden = !value;
      if (value === authenticated) return;
      authenticated = value; generation += 1;
      window.dispatchEvent(new CustomEvent('tokenmarket-tools-auth', {detail:{authenticated:value}}));
      if (value) refresh();
      else { if (disconnectConfirm.open) disconnectConfirm.close(); if (resultDialog.open) resultDialog.close(); if (reconnectDialog.open) reconnectDialog.close(); pendingReconnect = null; if (dialog.open) dialog.close(); if (disconnectDialog.open) disconnectDialog.close(); pendingDisconnect = null; entries = []; selectedTool = null; detailRoot.replaceChildren(); pendingConnect = null; if (connectDialog.open) connectDialog.close(); list.replaceChildren(); feedback(''); }
    },
  };
})();
