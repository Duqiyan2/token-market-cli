/* Navigation is presentation only. Native commands retain session and credential ownership. */
(() => {
  let identity = '';
  let detailTrigger = null;
  let listScroll = 0;
  let currentPage = '';
  function placeSettings() {
    const host = document.getElementById(identity ? 'app-content' : 'settings-standalone-content');
    const settings = document.getElementById('settings-page');
    if (settings.parentElement !== host) host.append(settings);
    document.getElementById('settings-standalone').hidden = Boolean(identity) || currentPage !== 'settings';
    document.getElementById('login').hidden = Boolean(identity) || currentPage === 'settings';
  }
  function show(page, title) {
    const known = ['tools', 'usage', 'account', 'help', 'detail', 'settings'];
    if (!known.includes(page)) page = 'tools';
    if (page === 'settings' && currentPage === 'settings') return true;
    const previous = currentPage;
    if (!window.dispatchEvent(new CustomEvent('tokenmarket-page-changing', {detail:{from:previous,to:page},cancelable:true}))) return false;
    currentPage = page;
    placeSettings();
    const drawer = document.getElementById('tool-detail-page');
    const content = document.getElementById('app-content');
    const wasOpen = !drawer.hidden;
    if (page === 'detail' && !wasOpen) { detailTrigger = document.activeElement; listScroll = content.scrollTop; }
    document.querySelectorAll('[data-view]').forEach(node => { node.hidden = node.dataset.view !== page; });
    if (page === 'detail' && !wasOpen) {
      drawer.focus({preventScroll:true});
      content.scrollTop = 0;
    } else if (wasOpen && page === 'tools') {
      if (detailTrigger?.isConnected) detailTrigger.focus({preventScroll:true});
      content.scrollTop = listScroll;
      detailTrigger = null;
    }
    document.querySelectorAll('.primary-nav [data-page]').forEach(node => {
      const active = node.dataset.page === (page === 'detail' ? 'tools' : page);
      node.classList.toggle('active', active);
      if (active) node.setAttribute('aria-current', 'page'); else node.removeAttribute('aria-current');
    });
    document.querySelectorAll('#signed-in [data-desktop-settings]').forEach(node => {
      node.classList.toggle('active', page === 'settings');
      if (page === 'settings') node.setAttribute('aria-current', 'page'); else node.removeAttribute('aria-current');
    });
    document.getElementById('breadcrumb').textContent = `工作台 / ${page === 'detail' ? `工具管理 / ${title || '工具详情'}` : title || ({tools:'工具管理', usage:'用量统计', account:'账号与空间', help:'使用帮助', settings:'应用设置'})[page]}`;
    if (page !== 'detail' && !(wasOpen && page === 'tools')) content.scrollTop = 0;
    window.dispatchEvent(new CustomEvent('tokenmarket-page-changed', {detail:{from:previous,to:page}}));
    return true;
  }
  document.addEventListener('click', event => {
    const button = event.target.closest('[data-page]');
    if (button) show(button.dataset.page);
  });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && !document.querySelector('dialog[open]') && !document.getElementById('tool-detail-page').hidden) show('tools');
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k' && identity && !document.querySelector('dialog[open]')) {
      event.preventDefault(); show('tools'); document.querySelector('.tools-search')?.focus();
    }
  });
  document.getElementById('focus-tool-search').onclick = () => { show('tools'); document.querySelector('.tools-search')?.focus(); };
  document.getElementById('settings-return-login').onclick = () => show('tools');
  window.TokenMarketShell = {
    show,
    get page() { return currentPage; },
    setAuthenticated(context) {
      const next = context ? String(context.user.id) : '';
      if (identity !== next) { identity = next; if (currentPage === 'settings') placeSettings(); else show('tools'); }
    },
  };
  show('tools');
})();
