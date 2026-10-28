/* Receives only the shared redacted connection summary. Native retains every credential. */
(() => {
  const root = document.getElementById('tools-region');
  const dialog = document.createElement('dialog');
  dialog.className = 'connection-test-dialog';
  dialog.innerHTML = '<h3>Connection Test · 测试连接</h3><p class="test-context"></p><p>本次会向该连接的模型发送一次真实请求，可能产生少量调用费用。计费使用此连接原有的 Key 和计费空间。</p><p>仅验证一次完整文本输出；不验证流式或工具调用，也不会更改 Agent 配置。</p><label class="test-model-label">本次测试模型 <select class="test-model"></select></label><p class="test-feedback" role="status" aria-live="polite"></p><div class="tool-actions"><button class="test-cancel" type="button">取消</button><button class="primary test-confirm" type="button">确认费用并测试一次</button></div>';
  root.append(dialog);
  const selector = dialog.querySelector('.test-model');
  const confirm = dialog.querySelector('.test-confirm');
  const cancel = dialog.querySelector('.test-cancel');
  const feedback = dialog.querySelector('.test-feedback');
  let pending = null;
  let busy = false;
  let generation = 0;
  window.TokenMarketConnectionTests = {get busy() { return busy; }};
  const text = (tag, value) => { const element = document.createElement(tag); element.textContent = value; return element; };
  cancel.onclick = () => { if (!busy) { pending = null; dialog.close(); } };
  dialog.addEventListener('cancel', event => { if (busy) event.preventDefault(); else pending = null; });
  selector.onchange = () => { confirm.disabled = busy || !selector.value; };
  window.addEventListener('tokenmarket-test-request', event => {
    if (busy) return;
    const entry = event.detail;
    if (!entry?.connection?.keyId || entry.connection.state === 'legacy') return;
    pending = entry;
    dialog.querySelector('.test-context').textContent = `${entry.name} · 所属账号：${entry.connection.owner.name} · 计费空间：${entry.connection.boundWorkspace.name} · 协议：${entry.connection.protocol}`;
    selector.replaceChildren();
    const single = entry.id === 'claude-code' || entry.id === 'codex';
    if (!single) { const option = text('option', '请选择一个已配置模型'); option.value = ''; selector.append(option); }
    (single ? ['glm-5.3'] : entry.connection.models).forEach(model => { const option = text('option', model); option.value = model; selector.append(option); });
    selector.disabled = single;
    confirm.disabled = !selector.value;
    cancel.disabled = false;
    cancel.textContent = '取消';
    confirm.textContent = '确认费用并测试一次';
    feedback.textContent = '';
    dialog.showModal();
  });
  confirm.onclick = async () => {
    if (!pending || busy || !selector.value) return;
    const entry = pending;
    const current = generation;
    busy = true;
    confirm.disabled = true; cancel.disabled = true; selector.disabled = true;
    feedback.textContent = '正在测试一次文本输出…';
    window.dispatchEvent(new CustomEvent('tokenmarket-test-busy'));
    try {
      const result = await window.__TAURI__.core.invoke('test_tool_connection', {tool:entry.id,expectedKeyId:entry.connection.keyId,request:{confirmed:true,model:selector.value}});
      if (current !== generation) return;
      feedback.textContent = `${result.passed ? '文本输出测试通过' : '文本输出测试失败'}：${result.detail}`;
      pending = null;
      confirm.textContent = '测试已完成';
    } catch (error) {
      if (current !== generation) return;
      feedback.textContent = String(error);
      pending = null;
      confirm.textContent = '测试未完成';
    } finally {
      busy = false;
      window.dispatchEvent(new CustomEvent('tokenmarket-test-busy'));
      if (current === generation) { cancel.disabled = false; cancel.textContent = '关闭'; await window.TokenMarketTools.refresh(true); }
    }
  };
  window.addEventListener('tokenmarket-tools-auth', () => { generation += 1; pending = null; if (dialog.open) dialog.close(); });
})();
