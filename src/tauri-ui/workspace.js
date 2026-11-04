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
  menuDialog.className = 'workspace-dialog workspace-switch-dialog';
  menuDialog.setAttribute('aria-label', '账号与空间');
  document.body.append(menuDialog);
  const profileMenu = document.createElement('dialog');
  profileMenu.id = 'account-menu';
  profileMenu.className = 'workspace-dialog account-menu';
  profileMenu.setAttribute('aria-label', '账号菜单');
  profileMenu.setAttribute('aria-modal', 'false');
  profileMenu.tabIndex = -1;
  document.body.append(profileMenu);
  function glyph(name) {
    const paths = {
      user: '<circle cx="12" cy="8" r="3"/><path d="M5 21v-2a7 7 0 0 1 14 0v2"/>',
      workspace: '<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M4 10h16M10 10v10"/>',
      settings: '<path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/>',
      switch: '<path d="M4 8h16l-4-4M20 16H4l4 4"/>',
      logout: '<path d="M10 4H5v16h5M10 12h11l-4-4m4 4-4 4"/>',
      external: '<path d="M14 4h6v6M20 4l-9 9M10 4H4v16h16v-6"/>',
      wallet: '<rect x="3" y="6" width="18" height="15" rx="2"/><path d="M3 6V3h14v3M16 11h5v5h-5z"/>',
      token: '<path d="m12 3 8 5v8l-8 5-8-5V8zM8 10l4 2 4-2m-4 2v5"/>',
      close: '<path d="m6 6 12 12M18 6 6 18"/>',
    };
    const node = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    node.setAttribute('viewBox', '0 0 24 24'); node.setAttribute('aria-hidden', 'true');
    node.innerHTML = paths[name] || paths.workspace;
    return node;
  }
  function closeProfile(restoreFocus = false) {
    if (!profileMenu.open) return;
    profileMenu.close();
    const trigger = profileRoot.querySelector('button');
    trigger?.setAttribute('aria-expanded', 'false');
    if (restoreFocus) trigger?.focus({preventScroll:true});
  }
  function positionProfile() {
    const trigger = profileRoot.querySelector('button');
    if (!trigger || !profileMenu.open) return;
    const rect = trigger.getBoundingClientRect(), height = profileMenu.getBoundingClientRect().height;
    profileMenu.style.left = `${Math.max(8, Math.min(rect.left, innerWidth - profileMenu.offsetWidth - 8))}px`;
    const top = rect.top > innerHeight / 2 ? rect.top - height - 8 : rect.bottom + 8;
    profileMenu.style.top = `${Math.max(8, Math.min(top, innerHeight - height - 8))}px`;
  }
  document.addEventListener('pointerdown', event => {
    if (profileMenu.open && !profileMenu.contains(event.target) && !profileRoot.contains(event.target)) closeProfile();
  });
  document.addEventListener('keydown', event => {
    if (!profileMenu.open) return;
    if (event.key === 'Escape') { event.preventDefault(); closeProfile(true); }
    if (!['ArrowDown','ArrowUp','Home','End'].includes(event.key)) return;
    const items = [...profileMenu.querySelectorAll('.profile-menu-item')];
    if (!items.length) return;
    event.preventDefault();
    const index = items.indexOf(document.activeElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : index < 0 ? (event.key === 'ArrowDown' ? 0 : items.length - 1) : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
    items[next].focus();
  });
  window.addEventListener('resize', positionProfile);
  function avatar(user) {
    const name = user.display_name || user.user_name || user.name || '账号';
    if (/^avatar_(?:[0-9]|1[01])$/.test(user.avatar_url ?? '')) {
      const image = document.createElement('img'); image.src = `system-avatars/${user.avatar_url}.png`; image.alt = ''; image.className = 'workspace-avatar'; return image;
    }
    return text('span', name.slice(0, 1).toUpperCase(), 'workspace-avatar');
  }
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
      const copy = text('span', '', 'workspace-choice-copy'); copy.append(text('strong', node.name), text('span', node.type === 'PERSONAL' || node.defaultWorkspace ? '个人空间' : roleLabel(node, snapshot.user)));
      const mark = text('span', '', 'workspace-choice-icon'); mark.append(glyph(node.type === 'PERSONAL' || node.defaultWorkspace ? 'user' : 'workspace'));
      item.append(mark, copy, text('small', active ? '当前使用' : '切换'));
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
    closeProfile();
    menuDialog.replaceChildren(text('h2', '切换当前空间'), text('p', '新接入使用所选空间，已有工具保留各自的计费空间。'));
    const list = text('div', '', 'workspace-choices'); workspaceOptions(list); menuDialog.append(list);
    menuDialog.append(button('关闭', () => menuDialog.close())); menuDialog.showModal();
  }
  function showProfile() {
    if (profileMenu.open) { closeProfile(true); return; }
    if (menuDialog.open) menuDialog.close();
    const user = snapshot?.user || sessionUser || {};
    const name = user.display_name || user.user_name || user.name || '账号';
    const header = text('div', '', 'profile-menu-header');
    const copy = text('div', '', 'profile-menu-identity');
    copy.append(text('strong', name), text('span', snapshot ? roleLabel(snapshot.current_workspace, user) : '账号信息暂不可用'));
    const close = button('', () => closeProfile(true)); close.className = 'profile-menu-close'; close.setAttribute('aria-label', '关闭账号菜单'); close.append(glyph('close'));
    header.append(avatar(user), copy, close);
    profileMenu.replaceChildren(header);
    const actions = text('div', '', 'profile-menu-actions');
    function item(label, icon, action, danger = false) {
      const node = button('', () => { closeProfile(); action(); });
      node.className = `profile-menu-item${danger ? ' danger' : ''}`;
      node.append(glyph(icon), text('span', label)); actions.append(node); return node;
    }
    if (snapshot) {
      item('账号与空间', 'workspace', () => window.TokenMarketShell.show('account'));
      item('个人设置', 'settings', () => void open('profile')).append(glyph('external'));
      item('切换账号', 'switch', () => document.getElementById('change-account').click());
    } else item('重新读取账号', 'switch', () => void refresh());
    item('退出登录', 'logout', () => document.getElementById('logout').click(), true);
    profileMenu.append(actions);
    profileMenu.style.left = '8px'; profileMenu.style.top = '8px';
    profileMenu.show(); positionProfile();
    profileRoot.querySelector('button')?.setAttribute('aria-expanded', 'true');
    profileMenu.focus({preventScroll:true});
  }
  function render() {
    closeProfile();
    root.replaceChildren(); accountRoot.replaceChildren(); profileRoot.replaceChildren();
    if (!snapshot) {
      root.append(text('p', '正在读取账号与空间…', 'workspace-status'));
      accountRoot.append(text('p', '正在读取账号与空间…', 'workspace-status')); const profile = button(sessionUser?.name || '账号', showProfile); profile.className = 'workspace-profile-button'; profile.setAttribute('aria-label', '账号菜单'); profileRoot.append(profile); return;
    }
    const current = snapshot.current_workspace, user = snapshot.user;
    const name = user.display_name || user.user_name || user.name || '账号';
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
    profile.setAttribute('aria-haspopup', 'dialog'); profile.setAttribute('aria-controls', profileMenu.id); profile.setAttribute('aria-expanded', 'false');
    profile.append(avatar(user), copy, profileChevron); profileRoot.append(profile);
    const account = text('header', '', 'account-profile');
    const userCopy = text('div', '', 'account-profile-copy');
    userCopy.append(text('h2', name));
    const meta = text('div', '', 'account-profile-meta');
    meta.append(text('span', roleLabel(current, user), 'account-role'));
    if (user.email) meta.append(text('span', user.email));
    userCopy.append(meta);
    const profileAction = button('个人设置', () => open('profile')); profileAction.className = 'quiet account-edit'; profileAction.append(glyph('external'));
    account.append(avatar(user), userCopy, profileAction); accountRoot.append(account);

    const overview = text('section', '', 'account-overview');
    const workspace = text('div', '', 'account-current'); workspace.append(text('h2', '当前空间'));
    const workspaceTitle = text('div', '', 'account-current-title');
    const workspaceIcon = text('span', '', 'account-space-icon'); workspaceIcon.append(glyph(current.type === 'PERSONAL' || current.defaultWorkspace ? 'user' : 'workspace'));
    const workspaceName = text('div', '', 'account-current-copy'); workspaceName.append(text('strong', current.name), text('span', current.type === 'PERSONAL' || current.defaultWorkspace ? '个人空间' : roleLabel(current, user)));
    workspaceTitle.append(workspaceIcon, workspaceName); workspace.append(workspaceTitle);
    const switchAction = button('切换空间', showSpaces); switchAction.className = 'account-switch-action';
    workspace.append(text('p', '新接入的工具使用这个空间，已有连接保持各自的计费归属。'), switchAction);
    const wallet = text('div', '', 'account-wallet'); wallet.append(text('h2', '可用余额'), text('div', amount, 'account-balance'), controls());
    const walletStatus = text('p', notice || snapshot.balance_error || '余额与充值以当前空间为准。', 'workspace-status'); walletStatus.dataset.feedback = ''; walletStatus.setAttribute('role', 'status'); wallet.append(walletStatus);
    overview.append(workspace, wallet); accountRoot.append(overview);

    const spaces = text('section', '', 'account-section'); spaces.append(text('h2', '可切换空间'));
    const list = text('div', '', 'workspace-choices account-spaces'); workspaceOptions(list); spaces.append(list); accountRoot.append(spaces);

    const management = text('section', '', 'account-section account-management');
    const managementHeading = text('div', '', 'account-section-heading'); managementHeading.append(text('h2', '平台管理'), text('span', '在浏览器中打开', 'caption')); management.append(managementHeading);
    const platformActions = text('div', '', 'account-management-list');
    [['tokens','Token 管理','管理平台访问凭据','token'], ['billing','钱包与账单','查看消费明细与充值记录','wallet'], ['workspaces','空间管理','管理空间与成员权限','workspace']].forEach(([action, label, description, icon]) => {
      if (!menuAllows(snapshot.menus ?? [], action)) return;
      const entry = button('', () => open(action)); entry.className = 'account-management-item'; entry.setAttribute('aria-label', label);
      const copy = text('span', '', 'account-management-copy'); copy.append(text('strong', label), text('span', description));
      entry.append(glyph(icon), copy, glyph('external')); platformActions.append(entry);
    });
    if (platformActions.childElementCount) { management.append(platformActions); accountRoot.append(management); }
    const session = text('footer', '', 'account-session'); const sessionCopy = text('div'); sessionCopy.append(text('strong', '当前会话'), text('p', '退出桌面账号不会取消已有工具的连接。'));
    const sessionActions = text('div', '', 'inline-actions'); sessionActions.append(button('切换账号', () => document.getElementById('change-account').click()));
    const logout = button('退出登录', () => document.getElementById('logout').click()); logout.className = 'quiet danger'; sessionActions.append(logout); session.append(sessionCopy, sessionActions); accountRoot.append(session);

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
    closeProfile();
    identity = next; sessionUser = context?.user || null; ++epoch; snapshot = null; switching = false; loading = false; notice = ''; root.replaceChildren(); accountRoot.replaceChildren(); profileRoot.replaceChildren(); if (menuDialog.open) menuDialog.close();
    if (identity) { render(); void refresh(); }
  } };
  window.addEventListener('focus', () => void refresh());
})();
