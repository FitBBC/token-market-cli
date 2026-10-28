/* Account/workspace UI receives public context only; management credentials stay in Rust. */
(() => {
  const root = document.getElementById('workspace-region');
  if (!root) return;
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  let identity = '', epoch = 0, snapshot = null, switching = false, loading = false, notice = '';
  const text = (tag, value, className) => { const node = document.createElement(tag); node.textContent = value ?? ''; if (className) node.className = className; return node; };
  function button(label, action) { const node = text('button', label); node.type = 'button'; node.onclick = action; return node; }
  function message(value) { notice = value; const node = root.querySelector('[data-feedback]'); if (node) node.textContent = value; }
  function menuAllows(nodes, action) {
    const keys = { tokens: ['api-token-list'], billing: ['billing-account-summary', 'billing-transaction-list', 'billing-recharge-order'], workspaces: ['workspace-list'] }[action] ?? [];
    const paths = { tokens: '/console/tokens', billing: '/console/billing', workspaces: '/console/spaces' };
    return nodes.some(node => node.visible !== false && (node.path === paths[action] || keys.some(key => node.pageKey === key || node.pageKeys?.includes(key)) || menuAllows(node.children ?? [], action)));
  }
  function flatten(nodes) { return nodes.flatMap(node => [node, ...flatten(node.children ?? [])]); }
  function roleLabel(current, user) {
    const role = current.roleCode ?? '';
    if (current.type !== 'PERSONAL' && !current.defaultWorkspace && ['OWNER', 'ADMIN', 'MEMBER'].some(suffix => role.endsWith(suffix))) return ({ OWNER: '空间所有者', ADMIN: '空间管理员', MEMBER: '空间成员' })[role.split('_').at(-1)];
    return user.role === 'SUPER_ADMIN' ? '超级管理员' : user.role === 'USER_ADMIN' ? '企业管理员' : '个人用户';
  }
  async function open(action) {
    try { await invoke('open_desktop_account_page', { action }); message('已在浏览器打开，返回应用时将刷新余额。'); }
    catch (error) { message(String(error)); }
  }
  function render() {
    root.replaceChildren();
    if (!snapshot) { root.append(text('p', '正在读取账号与空间…', 'workspace-status')); return; }
    const current = snapshot.current_workspace, user = snapshot.user;
    const header = text('header', '', 'workspace-header');
    const identityArea = text('div', '', 'workspace-identity');
    identityArea.append(text('span', 'Current Workspace', 'workspace-caption'), text('strong', current.name));
    const actions = text('div', '', 'workspace-actions');
    const amount = snapshot.balance ? `${snapshot.balance.currency} ${snapshot.balance.amount}` : '余额暂不可用';
    actions.append(text('span', amount, 'workspace-balance'));
    const refreshButton = button('刷新余额', refresh); refreshButton.disabled = switching || loading; actions.append(refreshButton);
    if (snapshot.can_recharge) { const recharge = button('充值 ↗', () => open('recharge')); recharge.disabled = switching; actions.append(recharge); }
    const dropdown = text('details', '', 'workspace-profile');
    const trigger = text('summary', '', 'workspace-avatar-trigger'); trigger.setAttribute('aria-label', '账号菜单');
    const name = user.display_name || user.user_name || '账号';
    const avatar = user.avatar_url;
    if (/^avatar_(?:[0-9]|1[01])$/.test(avatar ?? '')) { const image = document.createElement('img'); image.src = `system-avatars/${avatar}.png`; image.alt = ''; image.width = 34; image.height = 34; trigger.append(image); }
    else trigger.append(text('span', name.slice(0, 1).toUpperCase(), 'workspace-avatar'));
    trigger.append(text('span', name), text('span', '⌄'));
    const menu = text('div', '', 'workspace-menu'); menu.setAttribute('aria-label', '账号与空间');
    menu.append(text('strong', name), text('span', user.email || '', 'workspace-caption'), text('span', roleLabel(current, user), 'workspace-role'));
    const selector = text('details', '', 'workspace-selector'); selector.append(text('summary', `当前空间 · ${current.name}`));
    const list = text('div', '', 'workspace-options');
    const nodes = flatten(snapshot.switchable_workspaces).filter(node => node.switchable !== false && (!node.status || node.status === 'ACTIVE'));
    const unique = [...new Map(nodes.map(node => [node.id, node])).values()];
    const personal = unique.filter(node => node.type === 'PERSONAL' || node.defaultWorkspace);
    const primary = unique.filter(node => !personal.includes(node) && (!node.parentId || node.parentId === '0'));
    const other = unique.filter(node => !personal.includes(node) && !primary.includes(node));
    function option(node, nested = false) { const item = button(`${current.id === node.id ? '✓ ' : ''}${node.name}`, () => choose(node.id)); item.disabled = switching || node.id === current.id; if (nested) item.className = 'workspace-child'; list.append(item); }
    if (personal.length) { list.append(text('span', '个人空间', 'workspace-caption')); personal.forEach(node => option(node)); }
    if (primary.length || other.length) list.append(text('span', '企业空间', 'workspace-caption'));
    primary.forEach(node => { option(node); other.filter(child => child.parentId === node.id).forEach(child => option(child, true)); });
    other.filter(node => !primary.some(parent => parent.id === node.parentId)).forEach(node => option(node, true));
    if (menuAllows(snapshot.menus, 'workspaces')) list.append(button('空间管理 ↗', () => open('workspaces')));
    selector.append(list); menu.append(selector);
    menu.append(button('个人设置 ↗', () => open('profile')));
    if (menuAllows(snapshot.menus, 'tokens')) menu.append(button('Token 管理 ↗', () => open('tokens')));
    if (menuAllows(snapshot.menus, 'billing')) menu.append(button('钱包管理 ↗', () => open('billing')));
    menu.append(button('退出登录', async () => { try { await invoke('logout_desktop_session'); } catch (error) { message(String(error)); } }));
    dropdown.append(trigger, menu); actions.append(dropdown); header.append(identityArea, actions); root.append(header);
    const status = text('p', notice || snapshot.balance_error || '', 'workspace-status'); status.dataset.feedback = ''; status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite'); root.append(status);
  }
  async function refresh() {
    if (!identity || switching || loading) return;
    loading = true; const generation = epoch; if (snapshot) render();
    try { const next = await invoke('desktop_workspace_context'); if (generation === epoch && next) { snapshot = next; render(); } }
    catch (error) { if (generation === epoch) { if (!snapshot) root.replaceChildren(text('p', '无法读取空间。', 'workspace-status'), button('重试', refresh)); message(String(error)); } }
    finally { if (generation === epoch) { loading = false; if (snapshot) render(); } }
  }
  async function choose(workspaceId) {
    if (switching) return;
    notice = ''; switching = true; const generation = ++epoch; render();
    try { const next = await invoke('switch_desktop_workspace', { workspaceId }); if (generation === epoch && next) { snapshot = next; render(); window.dispatchEvent(new CustomEvent('current-workspace-changed', { detail: next.current_workspace })); } }
    catch (error) { if (generation === epoch) message(String(error)); }
    finally { if (generation === epoch) { switching = false; loading = false; render(); } }
  }
  window.TokenMarketWorkspace = { setAuthenticated(context) {
    const next = context ? `${context.user.id}:${context.expires_at}` : '';
    if (next === identity) return;
    identity = next; ++epoch; snapshot = null; switching = false; loading = false; notice = ''; root.replaceChildren();
    if (identity) { render(); void refresh(); }
  } };
  document.addEventListener('click', event => { const dropdown = root.querySelector('.workspace-profile'); if (dropdown && !dropdown.contains(event.target)) dropdown.open = false; });
  root.addEventListener('keydown', event => { if (event.key === 'Escape') { const dropdown = root.querySelector('.workspace-profile'); if (dropdown) { dropdown.open = false; dropdown.querySelector('summary')?.focus(); } } });
  window.addEventListener('focus', () => void refresh());
})();
