/* Navigation is presentation only. Native commands retain session and credential ownership. */
(() => {
  let identity = '';
  function show(page, title) {
    const known = ['tools', 'account', 'help', 'detail'];
    if (!known.includes(page)) page = 'tools';
    document.querySelectorAll('[data-view]').forEach(node => { node.hidden = node.dataset.view !== page; });
    document.querySelectorAll('.primary-nav [data-page]').forEach(node => {
      const active = node.dataset.page === (page === 'detail' ? 'tools' : page);
      node.classList.toggle('active', active);
      if (active) node.setAttribute('aria-current', 'page'); else node.removeAttribute('aria-current');
    });
    document.getElementById('breadcrumb').textContent = `工作台 / ${title || ({tools:'我的 Agent', account:'账号与空间', help:'使用帮助'})[page]}`;
    document.getElementById('app-content').scrollTop = 0;
  }
  document.addEventListener('click', event => {
    const button = event.target.closest('[data-page]');
    if (button) show(button.dataset.page);
  });
  document.addEventListener('keydown', event => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k' && identity && !document.querySelector('dialog[open]')) {
      event.preventDefault(); show('tools'); document.querySelector('.tools-search')?.focus();
    }
  });
  window.TokenMarketShell = {
    show,
    setAuthenticated(context) {
      const next = context ? String(context.user.id) : '';
      if (identity !== next) { identity = next; show('tools'); }
    },
  };
  show('tools');
})();
