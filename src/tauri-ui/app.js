const invoke = (...args) => window.__TAURI__.core.invoke(...args);
const el = id => document.getElementById(id);
let pending = null;
let starting = false;
function feedback(message, isError = false) {
  el('feedback').textContent = message;
  el('feedback').className = isError ? 'error' : '';
  el('session-feedback').textContent = message;
  el('session-feedback').hidden = !message;
  el('session-feedback').className = isError ? 'notice error' : 'notice';
}
function render(status) {
  pending = status.pending;
  const signedIn = Boolean(status.context);
  window.TokenMarketShell?.setAuthenticated(status.context);
  window.TokenMarketWorkspace?.setAuthenticated(status.context);
  el('login').hidden = signedIn;
  el('signed-in').hidden = !signedIn;
  window.TokenMarketTools?.setAuthenticated(signedIn);
  el('waiting').hidden = !pending && !starting && !status.exchanging;
  el('begin').hidden = Boolean(pending) || starting || Boolean(status.exchanging);
  el('begin').disabled = Boolean(status.restoring);
  if (signedIn && !status.error && ['正在恢复登录…','正在打开浏览器…','无法验证桌面会话，请重新登录'].includes(el('feedback').textContent)) feedback('');
  if (status.restoring && !signedIn) feedback('正在恢复登录…');
  el('copy').disabled = !pending;
  el('reopen').disabled = !pending;
  if (status.error) feedback(status.error, true);
}
el('begin').onclick = async () => {
  starting = true;
  feedback('正在打开浏览器…');
  render({ pending:null });
  try { pending = await invoke('begin_desktop_login'); feedback(''); }
  catch (error) { feedback(String(error), true); }
  finally { starting = false; await refresh(); }
};
el('copy').onclick = async () => {
  if (!pending) return;
  try { await navigator.clipboard.writeText(pending.authorize_url); feedback('登录链接已复制'); }
  catch (_) { feedback('复制失败，请点击“打开登录页面”重试', true); }
};
el('reopen').onclick = async () => {
  try { await invoke('reopen_desktop_login'); }
  catch (error) { feedback(String(error), true); }
};
el('cancel').onclick = async () => {
  try { await invoke('cancel_desktop_login'); starting = false; feedback('已取消等待'); await refresh(); }
  catch (error) { feedback(String(error), true); }
};
el('logout').onclick = async () => {
  try {
    const result = await invoke('logout_desktop_session');
    feedback(result.warning || '已退出登录，已有 Agent 配置保留', Boolean(result.warning));
    await refresh();
  } catch (error) { feedback(String(error), true); await refresh(); }
};
el('change-account').onclick = async () => {
  try { await invoke('logout_desktop_session'); await refresh(); el('begin').click(); }
  catch (error) { feedback(String(error), true); await refresh(); }
};
async function renewSession() {
  try { render(await invoke('refresh_desktop_session')); }
  catch (_) { feedback('无法验证桌面会话，请重新登录', true); }
}
window.addEventListener('focus', renewSession);
setInterval(renewSession, 5 * 60 * 1000);
async function refresh() {
  try { render(await invoke('desktop_login_status')); }
  catch (_) { feedback('无法读取登录状态，请重新打开应用', true); }
}
if (window.__TAURI__) {
  window.__TAURI__.event.listen('desktop-login-status', event => render(event.payload));
  refresh();
  setInterval(refresh, 1000);
} else {
  el('begin').disabled = true;
  feedback('请在 Token Market 桌面应用中打开此界面。');
}
