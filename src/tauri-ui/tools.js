/* Tools inventory owns only #tools-region; account/session/header remain separate. */
(() => {
  const root = document.getElementById('tools-region');
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const icons = {
    'claude-code': '<path d="M24 7v34M7 24h34M12 12l24 24M12 36l24-24M17 9l14 30M9 17l30 14M9 31l30-14M17 39L31 9"/>',
    codex: '<path d="m18 14-11 10 11 10m12-20 11 10-11 10m-3-24-6 28"/>',
    workbuddy: '<path d="m8 13 7 23 9-18 9 18 7-23"/>',
    'deepseek-harness': '<path d="M6 24c5 1 8-10 17-8 7 1 10 8 14 5l6-7-1 12c-4 3-5 11-16 11C15 37 9 33 6 24Z"/><path d="M16 20c2-6 7-8 11-7"/><circle cx="29" cy="23" r="1"/>',
    openclaw: '<path d="m15 22-7-5 3-9 7 7-3 7Zm18 0 7-5-3-9-7 7 3 7ZM20 18h8l4 9-4 13h-8l-4-13Z"/><path d="m17 28-7 5m21-5 7 5m-18 2-6 6m14-6 6 6"/>',
    opencode: '<path d="M9 10h30v28H9zM16 19l6 5-6 5m11 0h6"/>',
  };
  let authenticated = false;
  let loading = false;
  let inventoryPending = Promise.resolve();
  let connecting = false;
  let disconnecting = false;
  let pendingDisconnect = null;
  let generation = 0;
  let entries = [];
  root.innerHTML = '<div class="tools-heading"><div><h2>你的 Agent</h2><p>连接本机工具，开始使用 Token Market 模型。</p></div><button class="tools-refresh" type="button">重新检测</button></div><div class="tools-feedback" role="status" aria-live="polite"></div><div class="tools-list"></div>';
  const message = root.querySelector('.tools-feedback');
  const refreshButton = root.querySelector('.tools-refresh');
  const list = root.querySelector('.tools-list');
  function feedback(text, error = false) { message.textContent = text; message.classList.toggle('error', error); }
  function text(tag, value, className) {
    const element = document.createElement(tag);
    element.textContent = value;
    if (className) element.className = className;
    return element;
  }
  function render() {
    list.replaceChildren();
    entries.forEach(entry => {
      const row = document.createElement('article'); row.className = 'tool-row';
      const icon = document.createElement('div'); icon.className = `tool-icon tool-${entry.id}`;
      icon.innerHTML = `<svg viewBox="0 0 48 48" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2.7" stroke-linecap="round" stroke-linejoin="round">${icons[entry.id] || ''}</svg>`;
      row.append(icon);
      const content = document.createElement('div'); content.className = 'tool-details';
      const title = document.createElement('div'); title.className = 'tool-title';
      title.append(text('h3', entry.name), text('span', entry.installed ? '已安装' : '未安装', `tool-badge ${entry.installed ? 'installed' : ''}`));
      content.append(title);
      if (entry.connection) {
        content.append(text('p', entry.connection.state === 'legacy' ? '检测到旧版连接记录 · 账号与计费空间未知' : `${entry.connection.owner.name} · 计费空间：${entry.connection.boundWorkspace.name} · 已配置，未测试`, 'tool-description'));
        const models = document.createElement('details'); models.className = 'tool-model-list';
        models.append(text('summary', `${entry.connection.models.length} 个模型`));
        const items = document.createElement('ul');
        entry.connection.models.forEach(model => items.append(text('li', model)));
        models.append(items); content.append(models);
      } else {
        content.append(text('p', entry.installed ? '尚未接入 Token Market' : '安装后，点击重新检测即可接入', 'tool-description'));
      }
      row.append(content);
      const actions = document.createElement('div'); actions.className = 'tool-actions';
      if (!entry.installed) {
        const download = text('button', '官网下载', 'tool-download'); download.type = 'button';
        download.onclick = async () => {
          try { await invoke('download_tool', {tool:entry.id}); feedback('已打开官网。安装完成后点击“重新检测”。'); }
          catch (error) { feedback(String(error), true); }
        };
        actions.append(download);
      }
      const connect = text('button', entry.connection ? '重新接入' : '接入', 'primary tool-connect');
      connect.type = 'button'; connect.disabled = !entry.canConnect || loading || connecting || disconnecting || (entry.connection?.state === 'configured_untested');
      if (!entry.canConnect) connect.title = '请先安装此工具并重新检测';
      connect.onclick = async () => {
        connect.disabled = true;
        const actionGeneration = generation;
        try {
          await invoke('prepare_tool_connect', {tool:entry.id});
          if (!authenticated || actionGeneration !== generation) return;
          const event = new CustomEvent('tokenmarket-connect-request', {detail:{tool:entry.id}, cancelable:true});
          if (window.dispatchEvent(event)) feedback('接入功能正在准备中。');
        } catch (error) { feedback(String(error), true); await refresh(); }
        finally { connect.disabled = !entry.canConnect || connecting || (entry.connection?.state === 'configured_untested'); }
      };
      actions.append(connect);
      if (entry.connection && entry.connection.state !== 'legacy') {
        const menu = document.createElement('details'); menu.className = 'tool-menu';
        const toggle = text('summary', '操作 ▾'); menu.append(toggle);
        const options = document.createElement('div'); options.className = 'tool-menu-options';
        const launch = text('button', '启动 / 重新启动', 'tool-launch'); launch.type = 'button';
        launch.disabled = !entry.installed || connecting || disconnecting;
        launch.onclick = async () => {
          menu.open = false; const current = generation;
          try { await invoke('launch_connected_tool', {tool:entry.id}); if (authenticated && current === generation) feedback('已请求启动。现有会话请退出后重新启动以读取配置。'); }
          catch (error) { if (authenticated && current === generation) feedback(String(error), true); }
        };
        const disconnect = text('button', 'Disconnect · 取消接入', 'tool-disconnect'); disconnect.type = 'button';
        disconnect.disabled = connecting || disconnecting;
        disconnect.onclick = () => { menu.open = false; runDisconnect(entry, {revision:null,resolutions:[]}); };
        options.append(launch, disconnect); menu.append(options); actions.append(menu);
      }
      row.append(actions); list.append(row);
    });
  }
  async function refresh(waitForCurrent = false) {
    if (!authenticated) return;
    if (loading) { if (waitForCurrent === true) { await inventoryPending; return refresh(true); } return; }
    let finishInventory;
    inventoryPending = new Promise(resolve => { finishInventory = resolve; });
    loading = true; const current = generation;
    refreshButton.disabled = true; refreshButton.textContent = '检测中…';
    render();
    try {
      const next = await invoke('desktop_tool_inventory');
      if (authenticated && current === generation) { entries = next; feedback('安装状态已更新'); }
    } catch (error) { if (authenticated && current === generation) { entries = []; feedback(String(error), true); } }
    finally {
      loading = false; finishInventory(); refreshButton.disabled = false; refreshButton.textContent = '重新检测';
      if (authenticated && current === generation) render();
      else if (authenticated) refresh();
    }
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
        feedback(outcome.remoteDisabled ? `${entry.name} 已在本地取消接入，原 Key 已停用。` : `${entry.name} 已在本地取消接入；原 Key 未确认停用，请在平台 Key 管理中检查。`);
      }
    } catch (error) { if (authenticated && current === generation) feedback(String(error), true); }
    finally { disconnecting = false; if (authenticated && current === generation) render(); }
  }
  const supportedConnections = {'claude-code':'Claude Code', codex:'Codex CLI', 'deepseek-harness':'DeepSeek Harness',workbuddy:'WorkBuddy',openclaw:'OpenClaw',opencode:'OpenCode'};
  let launchTool = 'claude-code';
  const dialog = document.createElement('dialog');
  dialog.innerHTML = '<h3></h3><p class="restart-notice"></p><p>模型测试由你另行发起，可能产生少量费用。</p><div class="tool-actions"><button class="primary launch">打开终端运行 Claude Code</button><button class="close">完成</button></div><p class="launch-error" role="status"></p>';
  root.append(dialog);
  root.append(disconnectDialog);
  dialog.querySelector('.close').onclick = () => dialog.close();
  dialog.querySelector('.launch').onclick = async () => {
    try { await invoke('launch_connected_tool', {tool:launchTool}); dialog.close(); }
    catch(error) { dialog.querySelector('.launch-error').textContent = String(error); }
  };
  window.addEventListener('tokenmarket-connect-request', async event => {
    const tool = event.detail.tool;
    const name = supportedConnections[tool];
    if (!name) return;
    event.preventDefault();
    if (connecting || disconnecting) return;
    if (entries.find(entry => entry.id === tool)?.connection) {
      feedback('此工具已有配置记录，请通过后续的重新接入操作保留恢复基线。', true); return;
    }
    const actionGeneration = generation;
    connecting = true; render(); feedback(`正在取得当前空间 Key、读取模型目录并保存 ${name} 配置…`);
    try {
      const connected = await invoke('connect_tool', {tool:event.detail.tool});
      if (!authenticated || actionGeneration !== generation) return;
      await refresh(true);
      if (!authenticated || actionGeneration !== generation) return;
      const modelNotice = !['claude-code','codex'].includes(tool) ? `已导入 ${connected.models.length} 个兼容模型，尚未测试。` : '已配置 GLM-5.3，尚未测试。';
      feedback(`${name} ${modelNotice}`);
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
      dialog.querySelector('.launch-error').textContent = ''; dialog.showModal();
    } catch(error) { feedback(String(error), true); }
    finally { connecting = false; render(); }
  });
  refreshButton.onclick = refresh;
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
      if (value) refresh();
      else { if (dialog.open) dialog.close(); if (disconnectDialog.open) disconnectDialog.close(); pendingDisconnect = null; entries = []; list.replaceChildren(); feedback(''); }
    },
  };
})();
