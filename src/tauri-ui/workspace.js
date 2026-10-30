/* Account/workspace UI receives public context only; management credentials stay in Rust. */
(() => {
  const root = document.getElementById('workspace-region');
  if (!root) return;
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  let identity = '', sessionUser = null, epoch = 0, snapshot = null, switching = false, loading = false, notice = '';
  const text = (tag, value, className) => { const node = document.createElement(tag); node.textContent = value ?? ''; if (className) node.className = className; return node; };
  function button(label, action) { const node = text('button', label); node.type = 'button'; node.onclick = action; return node; }
  function message(value) { notice = value; document.querySelectorAll('[data-feedback]').forEach(node => { node.textContent = value; }); }
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
  const accountRoot = document.getElementById('workspace-account-region');
  const profileRoot = document.getElementById('sidebar-profile');
  const menuDialog = document.createElement('dialog');
  menuDialog.className = 'workspace-dialog';
  menuDialog.setAttribute('aria-label', '账号与空间');
  document.body.append(menuDialog);
  function avatar(user) {
    const name = user.display_name || user.user_name || '账号';
    if (/^avatar_(?:[0-9]|1[01])$/.test(user.avatar_url ?? '')) {
      const image = document.createElement('img'); image.src = `system-avatars/${user.avatar_url}.png`; image.alt = ''; image.className = 'workspace-avatar'; return image;
    }
    return text('span', name.slice(0, 1).toUpperCase(), 'workspace-avatar');
  }
  function row(label, value) { const node = text('div', '', 'info-row'); node.append(text('span', label), text('strong', value)); return node; }
  function formatBalance(amount) {
    // Keep decimal strings exact, including large balances; truncate without rounding.
    const [whole, fraction = ''] = String(amount).split('.');
    return `${whole}.${fraction.padEnd(2, '0').slice(0, 2)}`;
  }
  function controls() {
    const actions = text('div', '', 'inline-actions');
    const retry = button('刷新余额', () => { notice = ''; void refresh(); }); retry.dataset.workspaceRefresh = ''; retry.disabled = loading || switching;
    if (snapshot.can_recharge) { const recharge = button('充值', () => open('recharge')); recharge.className = 'primary'; recharge.disabled = switching; actions.append(recharge); }
    actions.append(retry); return actions;
  }
  function workspaceOptions(container) {
    const nodes = flatten(snapshot.switchable_workspaces ?? []).filter(node => node.switchable !== false && (!node.status || node.status === 'ACTIVE'));
    const unique = [...new Map(nodes.map(node => [node.id, node])).values()];
    const personal = unique.filter(node => node.type === 'PERSONAL' || node.defaultWorkspace);
    const primary = unique.filter(node => !personal.includes(node) && (!node.parentId || node.parentId === '0'));
    const other = unique.filter(node => !personal.includes(node) && !primary.includes(node));
    function option(node, nested = false) {
      const active = node.id === snapshot.current_workspace.id;
      const item = button('', () => { if (menuDialog.open) menuDialog.close(); void choose(node.id); });
      item.className = `workspace-choice${active ? ' active' : ''}${nested ? ' workspace-child' : ''}`;
      item.append(text('span', node.name), text('small', active ? '当前使用' : '切换'));
      item.disabled = switching || active; item.dataset.workspaceId = node.id; container.append(item);
    }
    if (personal.length) { container.append(text('span', '个人空间', 'workspace-caption')); personal.forEach(node => option(node)); }
    if (primary.length || other.length) container.append(text('span', '企业空间', 'workspace-caption'));
    primary.forEach(node => { option(node); other.filter(child => child.parentId === node.id).forEach(child => option(child, true)); });
    other.filter(node => !primary.some(parent => parent.id === node.parentId)).forEach(node => option(node, true));
    if (!unique.length) container.append(text('p', '当前没有可切换的空间。'));
  }
  function showSpaces() {
    if (!snapshot || switching) return;
    menuDialog.replaceChildren(text('h2', '切换当前空间'), text('p', '新接入使用所选空间，已有工具保留各自的计费空间。'));
    const list = text('div', '', 'workspace-choices'); workspaceOptions(list); menuDialog.append(list);
    menuDialog.append(button('关闭', () => menuDialog.close())); menuDialog.showModal();
  }
  function showProfile() {
    if (!snapshot) {
      menuDialog.replaceChildren(text('h2', sessionUser?.name || '账号'), text('p', '账号与空间暂时无法读取。'), button('重试', () => { menuDialog.close(); void refresh(); }), button('退出登录', () => { menuDialog.close(); document.getElementById('logout').click(); }));
      menuDialog.showModal(); return;
    }
    const name = snapshot.user.display_name || snapshot.user.user_name || '账号';
    menuDialog.replaceChildren(text('h2', name), text('p', roleLabel(snapshot.current_workspace, snapshot.user)));
    const actions = text('div', '', 'workspace-profile-actions');
    actions.append(button('账号与空间', () => { menuDialog.close(); window.TokenMarketShell.show('account'); }));
    actions.append(button('个人设置', () => { menuDialog.close(); void open('profile'); }));
    actions.append(button('切换账号', () => { menuDialog.close(); document.getElementById('change-account').click(); }));
    const logout = button('退出登录', () => { menuDialog.close(); document.getElementById('logout').click(); }); logout.className = 'danger'; actions.append(logout);
    actions.append(button('关闭', () => menuDialog.close())); menuDialog.append(actions); menuDialog.showModal();
  }
  function render() {
    root.replaceChildren(); accountRoot.replaceChildren(); profileRoot.replaceChildren();
    if (!snapshot) {
      root.append(text('p', '正在读取账号与空间…', 'workspace-status'));
      accountRoot.append(text('p', '正在读取账号与空间…', 'workspace-status')); const profile = button(sessionUser?.name || '账号', showProfile); profile.className = 'workspace-profile-button'; profile.setAttribute('aria-label', '账号菜单'); profileRoot.append(profile); return;
    }
    const current = snapshot.current_workspace, user = snapshot.user;
    const name = user.display_name || user.user_name || '账号';
    const amount = snapshot.balance ? `${snapshot.balance.currency === 'CNY' ? '¥' : `${snapshot.balance.currency} `}${formatBalance(snapshot.balance.amount)}` : '余额暂不可用';
    const header = text('header', '', 'workspace-header');
    const identityArea = text('div', '', 'workspace-identity');
    const switcher = button('', showSpaces); switcher.className = 'workspace-switch'; switcher.disabled = switching;
    const chevron = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    chevron.setAttribute('viewBox', '0 0 24 24'); chevron.setAttribute('aria-hidden', 'true'); chevron.setAttribute('focusable', 'false'); chevron.classList.add('workspace-switch-icon');
    chevron.innerHTML = '<path d="m6 9 6 6 6-6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>';
    switcher.append(text('span', current.name, 'workspace-switch-label'), chevron);
    switcher.setAttribute('aria-haspopup', 'dialog');
    identityArea.append(text('span', '当前空间 · 用于下一次接入', 'workspace-caption'), switcher);
    const balance = text('div', '', 'workspace-actions');
    const money = text('div', '', 'workspace-money'); money.append(text('span', '可用余额', 'workspace-caption'), text('strong', amount, 'workspace-balance'));
    balance.append(money, controls()); header.append(identityArea, balance); root.append(header);
    const status = text('p', notice || snapshot.balance_error || '', 'workspace-status'); status.dataset.feedback = ''; status.setAttribute('role', 'status'); root.append(status);
    const profile = button('', showProfile); profile.className = 'workspace-profile-button'; profile.setAttribute('aria-label', '账号菜单');
    const copy = text('span', '', 'workspace-profile-copy'); copy.append(text('strong', name), text('small', roleLabel(current, user)));
    const profileChevron = chevron.cloneNode(true); profileChevron.setAttribute('class', 'workspace-chevron');
    profile.setAttribute('aria-haspopup', 'dialog');
    profile.append(avatar(user), copy, profileChevron); profileRoot.append(profile);
    const grid = text('div', '', 'page-grid'), left = text('div'), right = text('div');
    const account = text('section', '', 'sheet');
    const title = text('div', '', 'account-identity'); const userCopy = text('div'); userCopy.append(text('h2', name), text('p', user.email || roleLabel(current, user))); title.append(avatar(user), userCopy);
    account.append(title, row('当前空间', current.name), row('账号身份', roleLabel(current, user)));
    const profileActions = text('div', '', 'inline-actions'); profileActions.append(button('个人设置', () => open('profile')), button('切换账号', () => document.getElementById('change-account').click())); account.append(profileActions); left.append(account);
    const spaces = text('section', '', 'sheet'); spaces.append(text('h2', '可切换空间')); const list = text('div', '', 'workspace-choices'); workspaceOptions(list); spaces.append(list, text('p', '切换后，新接入的工具使用所选空间。已有连接的计费空间保持不变。')); left.append(spaces);
    const wallet = text('section', '', 'sheet'); wallet.append(text('h2', '当前空间余额'), text('span', current.name, 'caption'), text('div', amount, 'account-balance'), controls());
    const walletStatus = text('p', notice || snapshot.balance_error || '余额与充值以当前空间为准。', 'workspace-status'); walletStatus.dataset.feedback = ''; walletStatus.setAttribute('role', 'status'); wallet.append(walletStatus); right.append(wallet);
    const management = text('section', '', 'sheet'); management.append(text('h2', '平台管理'));
    const platformActions = text('div', '', 'workspace-profile-actions');
    [['profile','个人设置'], ['tokens','Token 管理'], ['billing','钱包与账单'], ['workspaces','空间管理']].forEach(([action, label]) => {
      if (action === 'profile' || menuAllows(snapshot.menus ?? [], action)) platformActions.append(button(label, () => open(action)));
    });
    management.append(platformActions, text('p', '在浏览器中的平台控制台完成管理。', 'caption')); right.append(management);
    const logout = button('退出登录', () => document.getElementById('logout').click()); logout.className = 'quiet danger'; right.append(logout);
    grid.append(left, right); accountRoot.append(grid);
  }
  async function refresh() {
    if (!identity || switching || loading) return;
    loading = true; const generation = epoch; document.querySelectorAll('[data-workspace-refresh]').forEach(button => { button.disabled = true; });
    try { const next = await invoke('desktop_workspace_context'); if (generation === epoch && next && JSON.stringify(snapshot) !== JSON.stringify(next)) { const changed = snapshot && snapshot.current_workspace.id !== next.current_workspace.id; snapshot = next; render(); if (changed) window.dispatchEvent(new CustomEvent('current-workspace-changed', {detail:next.current_workspace})); } }
    catch (error) { if (generation === epoch) { if (!snapshot) { root.replaceChildren(text('p', '无法读取空间。', 'workspace-status'), button('重试', refresh)); accountRoot.replaceChildren(text('p', '无法读取账号与空间。'), button('重试', refresh)); } message(String(error)); } }
    finally { if (generation === epoch) { loading = false; document.querySelectorAll('[data-workspace-refresh]').forEach(button => { button.disabled = switching; }); } }
  }
  async function choose(workspaceId) {
    if (switching) return;
    notice = ''; switching = true; const generation = ++epoch; render();
    try { const next = await invoke('switch_desktop_workspace', { workspaceId }); if (generation === epoch && next) { snapshot = next; render(); window.dispatchEvent(new CustomEvent('current-workspace-changed', { detail: next.current_workspace })); } }
    catch (error) { if (generation === epoch) message(String(error)); }
    finally { if (generation === epoch) { switching = false; loading = false; render(); } }
  }
  window.TokenMarketWorkspace = { refresh, get current() { return snapshot; }, setAuthenticated(context) {
    const next = context ? String(context.user.id) : '';
    if (next === identity) return;
    identity = next; sessionUser = context?.user || null; ++epoch; snapshot = null; switching = false; loading = false; notice = ''; root.replaceChildren(); accountRoot.replaceChildren(); profileRoot.replaceChildren(); if (menuDialog.open) menuDialog.close();
    if (identity) { render(); void refresh(); }
  } };
  window.addEventListener('focus', () => void refresh());
})();
