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
  let connecting = false;
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
        content.append(text('span', entry.connection.models.join(' · '), 'tool-models'));
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
      connect.type = 'button'; connect.disabled = !entry.canConnect || loading || connecting || (entry.connection?.state === 'configured_untested');
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
      actions.append(connect); row.append(actions); list.append(row);
    });
  }
  async function refresh() {
    if (!authenticated || loading) return;
    loading = true; const current = generation;
    refreshButton.disabled = true; refreshButton.textContent = '检测中…';
    render();
    try {
      const next = await invoke('desktop_tool_inventory');
      if (authenticated && current === generation) { entries = next; feedback('安装状态已更新'); }
    } catch (error) { if (authenticated && current === generation) { entries = []; feedback(String(error), true); } }
    finally {
      loading = false; refreshButton.disabled = false; refreshButton.textContent = '重新检测';
      if (authenticated && current === generation) render();
      else if (authenticated) refresh();
    }
  }
  const dialog = document.createElement('dialog');
  dialog.innerHTML = '<h3>Claude Code 已接入</h3><p>已配置 GLM-5.3，尚未测试。现有 Claude Code 会话需要退出后重新启动，新终端将读取接入配置。</p><p>模型测试由你另行发起，可能产生少量费用。</p><div class="tool-actions"><button class="primary launch">打开终端运行 Claude Code</button><button class="close">完成</button></div><p class="launch-error" role="status"></p>';
  root.append(dialog);
  dialog.querySelector('.close').onclick = () => dialog.close();
  dialog.querySelector('.launch').onclick = async () => {
    try { await invoke('launch_connected_tool', {tool:'claude-code'}); dialog.close(); }
    catch(error) { dialog.querySelector('.launch-error').textContent = String(error); }
  };
  window.addEventListener('tokenmarket-connect-request', async event => {
    if (event.detail.tool !== 'claude-code') return;
    event.preventDefault();
    if (connecting) return;
    if (entries.find(entry => entry.id === 'claude-code')?.connection) {
      feedback('此工具已有配置记录，请通过后续的重新接入操作保留恢复基线。', true); return;
    }
    const actionGeneration = generation;
    connecting = true; render(); feedback('正在取得当前空间 Key、读取模型目录并保存 Claude Code 配置…');
    try {
      await invoke('connect_tool', {tool:event.detail.tool});
      if (!authenticated || actionGeneration !== generation) return;
      await refresh();
      if (!authenticated || actionGeneration !== generation) return;
      feedback('Claude Code 已配置 GLM-5.3，尚未测试。');
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
      else { if (dialog.open) dialog.close(); entries = []; list.replaceChildren(); feedback(''); }
    },
  };
})();
