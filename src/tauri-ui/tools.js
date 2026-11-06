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
  let pendingRecovery = null;
  let generation = 0;
  let installing = false, batching = false, batchStop = false;
  let checking = false, updatePending = null, checkRequested = false, checkExplicitRequested = false;
  let updates = new Map();
  let entries = [];
  const isVisibleTool = tool => tool !== 'dumate-enterprise';
  const expandedInstallations = new Set();
  let selectedTool = null, filter = 'all', search = '';
  const detailRoot = document.getElementById('tool-detail-region');
  const restartConfirm = document.createElement('dialog');
  restartConfirm.className = 'dumate-restart-confirm';
  restartConfirm.innerHTML = '<h3>强制重启搭子</h3><p>所有正在运行的任务都将被中断，定时任务也将无法调度。</p><div class="tool-actions"><button class="danger force-restart-dumate" type="button">强制重启搭子</button><button class="cancel-restart-dumate" type="button" autofocus>取消</button></div>';
  document.body.append(restartConfirm);
  let resolveRestartConfirmation = null;
  function closeRestartConfirmation(confirmed=false) {
    const resolve=resolveRestartConfirmation;resolveRestartConfirmation=null;
    if (restartConfirm.open) restartConfirm.close();
    resolve?.(confirmed);
  }
  restartConfirm.querySelector('.force-restart-dumate').onclick=()=>closeRestartConfirmation(true);
  restartConfirm.querySelector('.cancel-restart-dumate').onclick=()=>closeRestartConfirmation();
  restartConfirm.addEventListener('cancel',event=>{event.preventDefault();closeRestartConfirmation();});
  let restartingDuMate=false;
  async function restartDuMate(button,status) {
    if (!authenticated || restartingDuMate) return;
    const current=generation, token=Symbol(), origin=status.closest('dialog');status._restartToken=token;
    const active=()=>authenticated && current===generation && status._restartToken===token && origin?.open;
    restartingDuMate=true;button.disabled=true;button.textContent='正在准备重启…';
    status.textContent='';
    try {
      let result=await invoke('restart_dumate',{force:false});
      if (!active()) return;
      if (result.state==='confirmation_required') {
        const confirmed=await new Promise(resolve=>{resolveRestartConfirmation=resolve;restartConfirm.showModal();});
        if (!confirmed || !active()) return;
        button.textContent='正在重启搭子…';
        result=await invoke('restart_dumate',{force:true});
      }
      if (active()) origin.close();
    } catch (error) {
      if (active()) status.textContent=String(error);
    } finally {
      restartingDuMate=false;button.disabled=false;button.textContent='重启搭子';
    }
  }
  let restartingQwenWork=false;
  async function restartQwenWork(button,status) {
    if (!authenticated || restartingQwenWork) return;
    const current=generation, token=Symbol(), origin=status.closest('dialog');status._restartToken=token;
    const active=()=>authenticated && current===generation && status._restartToken===token && origin?.open;
    restartingQwenWork=true;button.disabled=true;button.textContent='请在千问办公中确认退出…';status.textContent='';
    try {
      const result=await invoke('restart_qwenwork',{});
      if (!active()) return;
      if (['opened','restarted'].includes(result.state)) origin.close();
      else status.textContent=result.message || '未完成重启，请手动重启千问办公。';
    } catch(error) {
      if (active()) status.textContent=String(error);
    } finally {
      restartingQwenWork=false;button.disabled=false;button.textContent='重启千问办公';
    }
  }
  const openToolMenus = () => document.querySelectorAll('.tool-menu[open], .tool-lifecycle-menu[open]');
  function closeToolMenusOutside(target) {
    openToolMenus().forEach(menu => { if (!target || !menu.contains(target)) menu.open = false; });
  }
  document.addEventListener('pointerdown', event => closeToolMenusOutside(event.target));
  document.addEventListener('mousedown', event => {
    // WebKit can focus the containing page instead of the clicked button.
    // Keep focus in the disclosure until click runs, so focusin cannot close it first.
    if (event.button === 0 && event.target.closest('.tool-menu[open] button, .tool-lifecycle-menu[open] button')) event.preventDefault();
  });
  document.addEventListener('focusin', event => closeToolMenusOutside(event.target));
  document.addEventListener('keydown', event => {
    if (event.key !== 'Escape' || document.querySelector('dialog[open]')) return;
    const menus = [...openToolMenus()];
    if (!menus.length) return;
    const current = menus.find(menu => menu.contains(document.activeElement)) || menus[0];
    event.preventDefault(); event.stopPropagation();
    closeToolMenusOutside();
    current.querySelector('summary')?.focus({preventScroll:true});
  }, true);
  window.addEventListener('tokenmarket-page-changed', () => closeToolMenusOutside());
  root.innerHTML = '<div class="tools-controls"><div class="tools-filters" role="group" aria-label="按工具状态筛选"><button data-tool-filter="all" class="selected">全部</button><button data-tool-filter="connected">已接入</button><button data-tool-filter="available">待接入</button><button data-tool-filter="uninstalled">未安装</button></div><input type="search" class="tools-search" placeholder="搜索工具或模型" aria-label="搜索工具或模型"></div><div class="tools-feedback" role="status" aria-live="polite"></div><div class="tools-table-heading" aria-hidden="true"><span>工具名称</span><span class="install-column">版本与安装</span><span>接入状态</span><span class="billing-column">计费空间</span><span>操作</span></div><div class="tools-list"></div><p class="tools-count"></p>';
  const searchInput = root.querySelector('.tools-search');
  searchInput.oninput = () => { search = searchInput.value.toLowerCase().trim(); render(); };
  root.querySelectorAll('[data-tool-filter]').forEach(button => { button.onclick = () => { filter = button.dataset.toolFilter; render(); }; });
  const message = root.querySelector('.tools-feedback');
  const refreshButton = document.getElementById('detect-tools');
  const upgradeAllButton = document.getElementById('upgrade-all-tools');
  upgradeAllButton.title = '仅统计可直接升级的工具；手动更新需在原安装渠道操作';
  const list = root.querySelector('.tools-list');
  const updateStatus = text('p', '', 'tools-updates-status');
  updateStatus.setAttribute('role', 'status'); updateStatus.setAttribute('aria-live', 'polite');
  root.querySelector('.tools-controls').before(updateStatus);
  function feedback(text, error = false) { message.textContent = text; message.classList.toggle('error', error); const detailMessage = detailRoot.querySelector('.tool-operation-status'); if (detailMessage) { detailMessage.textContent = text; detailMessage.classList.toggle('error', error); } }
  const resultDialog = document.createElement('dialog');
  resultDialog.className = 'tool-result-dialog';
  resultDialog.innerHTML = '<h3></h3><p class="tool-result-detail" role="status"></p><p class="dumate-restart-status error" role="alert"></p><form method="dialog" class="tool-actions"><button class="primary restart-dumate" type="button" hidden>重启搭子</button><button class="primary restart-qwenwork" type="button" hidden>重启千问办公</button><button class="result-close" type="submit">知道了</button></form>';
  document.body.append(resultDialog);
  resultDialog.addEventListener('close',()=>{
    const status=resultDialog.querySelector('.dumate-restart-status');
    if (status._restartToken) { status._restartToken=null;closeRestartConfirmation(); }
  });
  resultDialog.querySelector('.restart-dumate').onclick=()=>restartDuMate(resultDialog.querySelector('.restart-dumate'),resultDialog.querySelector('.dumate-restart-status'));
  resultDialog.querySelector('.restart-qwenwork').onclick=()=>restartQwenWork(resultDialog.querySelector('.restart-qwenwork'),resultDialog.querySelector('.dumate-restart-status'));
  function showResult(title, detail, canRestartDuMate=false, canRestartQwenWork=false) {
    resultDialog.querySelector('h3').textContent = title;
    resultDialog.querySelector('.tool-result-detail').textContent = detail;
    resultDialog.querySelector('.restart-dumate').hidden=!canRestartDuMate;
    resultDialog.querySelector('.restart-qwenwork').hidden=!canRestartQwenWork;
    resultDialog.querySelector('.dumate-restart-status')._restartToken=null;
    resultDialog.querySelector('.dumate-restart-status').textContent='';
    resultDialog.querySelector('.result-close').textContent=canRestartDuMate ? '稍后手动重启搭子' : canRestartQwenWork ? '稍后手动重启千问办公' : '知道了';
    if (!resultDialog.open) resultDialog.showModal();
  }
  function showConnectFailure() { feedback(''); showResult('接入失败', '请联系管理员。'); }
  async function duMateSavedStatus() {
    try {
      const running=await invoke('is_dumate_running');
      return {canRestart:running===true,notice:running===true ? '已保存，重启搭子后生效。' : '已保存，下次启动搭子时生效。'};
    } catch {
      return {canRestart:false,notice:'已保存，下次启动搭子时生效。暂时无法确认搭子是否运行，若正在运行，请完全退出后重新打开。'};
    }
  }
  async function qwenWorkSavedStatus() {
    try {
      const running=await invoke('is_qwenwork_running');
      return {canRestart:running===true,notice:running===true ? '已保存，重启千问办公后生效。重启时请在千问办公中确认退出。' : '已保存，下次启动千问办公时生效。'};
    } catch {
      return {canRestart:false,notice:'已保存，下次启动千问办公时生效。暂时无法确认千问办公是否运行，若正在运行，请完全退出后重新打开。'};
    }
  }
  function text(tag, value, className) {
    const element = document.createElement(tag);
    element.textContent = value;
    if (className) element.className = className;
    return element;
  }
  function connectionLabel(entry) {
    if (!entry.connection) return '未接入';
    return ({environment_mismatch:'已接入其他环境', configuration_changed:'配置已改变或不可用', configured_untested:'已配置，未测试', test_passed:'接口文本测试通过', test_failed:'接口文本测试失败'})[entry.connection.state] || '已配置，未测试';
  }
  const currentConnection = entry => Boolean(entry.connection) && entry.connection.state !== 'environment_mismatch';
  const testBlocked = entry => ['configuration_changed','environment_mismatch'].includes(entry.connection?.state);
  function stateBadge(entry) {
    if (!entry.installed && !entry.connection) {
      const state = text('span', '—', 'tool-state not-applicable'); state.title = '安装后可接入'; state.setAttribute('aria-label', '尚无连接，安装后可接入'); return state;
    }
    return text('span', connectionLabel(entry), `tool-state ${entry.connection?.state || 'unconnected'}`);
  }
  function toolIcon(entry) {
    const icon = text('div', '', 'tool-icon'); const image = document.createElement('img');
    image.src = `assets/tools/${entry.id === 'dumate-enterprise' ? 'dumate' : entry.id}.${({'workbuddy':'svg',openclaw:'svg',codex:'ico','deepseek-harness':'ico','grok-build':'svg',hermes:'svg',pi:'svg','minimax-code':'svg','oh-my-pi':'svg','kimi-code':'svg'})[entry.id] || 'png'}`;
    image.alt = ''; icon.append(image); return icon;
  }
  function actionButton(label, className, action, disabled = false) {
    const button = text('button', label, className); button.type = 'button'; button.disabled = disabled; button.onclick = action; return button;
  }
  function withIcon(button, direction = 'up') {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('aria-hidden', 'true'); svg.classList.add('tool-action-icon');
    svg.innerHTML = direction === 'external' ? '<path d="M14 3h7v7m0-7L11 13M10 5H4v15h15v-6"/>' : direction === 'up' ? '<circle cx="12" cy="12" r="9"/><path d="m8 12 4-4 4 4M12 8v9"/>' : '<path d="M12 3v12m-4-4 4 4 4-4M4 16v5h16v-5"/>';
    button.prepend(svg); return button;
  }
  const locations = entry => updates.get(entry.id)?.installations || (Array.isArray(entry.installations) ? entry.installations : []);
  const sourceLabel = method => ({npm:'npm',Homebrew:'Homebrew',pnpm:'pnpm',Bun:'bun',pip:'pip',Scoop:'Scoop'})[method] || '';
  const isPresent = entry => entry.installed || locations(entry).length > 0 || updates.get(entry.id)?.state === 'broken';
  const matchesFilter = (entry, kind) => kind === 'all'
    || kind === 'connected' && currentConnection(entry)
    || kind === 'available' && entry.installed && !currentConnection(entry)
    || kind === 'uninstalled' && !isPresent(entry);
  const selectedLocation = entry => locations(entry).find(location => location.selected) || locations(entry)[0];
  function installationSummary(entry) {
    const source = selectedLocation(entry);
    const summary = text('div', '', 'tool-installation-summary');
    if (source) {
      const line = text('div', '', 'tool-location-line');
      const path = text('code', source.path, 'tool-location-path'); path.title = source.path; line.append(path); summary.append(line);
      if (locations(entry).length > 1) {
        const toggle = actionButton(`另有 ${locations(entry).length-1} 处安装`, 'quiet tool-installations-toggle', () => {
          if (expandedInstallations.has(entry.id)) expandedInstallations.delete(entry.id); else expandedInstallations.add(entry.id);
          render(); root.querySelector(`[data-installation-toggle="${entry.id}"]`)?.focus();
        });
        toggle.dataset.installationToggle = entry.id; toggle.setAttribute('aria-expanded',String(expandedInstallations.has(entry.id))); toggle.setAttribute('aria-controls',`tool-installations-${entry.id}`); summary.append(toggle);
      }
    } else {
      summary.append(text('span', entry.installed ? '安装路径暂未获取' : entry.installationMethod || '自动安装命令行工具', 'tool-planned-method'));
    }
    return summary;
  }
  function installationPanel(entry) {
    const panel = text('div', '', 'tool-installation-list'); panel.id = `tool-installations-${entry.id}`; panel.setAttribute('role','group'); panel.setAttribute('aria-label',`${entry.name} 的安装位置`);
    locations(entry).forEach(source => {
      const row = text('div', '', 'tool-installation-location');
      row.append(text('span', sourceLabel(source.method), 'tool-install-method'),text('code', source.path, 'tool-path-value'));
      if (source.isPathDefault || (source.isPathDefault === undefined && source.selected)) row.append(text('span','终端默认','tool-current-installation'));
      if (source.version) row.append(text('span',versionDisplay(source.version),'tool-location-version'));
      if (source.error) row.append(text('span',source.error,'tool-location-error'));
      panel.append(row);
    });
    panel.append(text('p','版本检测与升级使用标记的入口；其他应用可能选择不同的安装。','tool-installation-note'));
    return panel;
  }
  function pathRow(label,path) {
    const row = text('div', '', 'info-row tool-path-row'); row.append(text('span',label),text('code',path || '暂未获取','tool-path-value')); return row;
  }
  function installationDetails(entry) {
    const sheet = text('section', '', 'sheet tool-files-sheet'); sheet.append(text('h2','安装与配置'));
    const source = selectedLocation(entry);
    sheet.append(detailRow(entry.installed ? '安装方式' : '安装方案', entry.installed ? sourceLabel(source?.method) || '来源未确认' : entry.installationMethod || '暂未获取'));
    if (source) {
      sheet.append(pathRow('当前入口',source.path));
      if (source.resolvedPath && source.resolvedPath !== source.path) sheet.append(pathRow('实际文件',source.resolvedPath));
    }
    sheet.append(pathRow('配置目录',entry.configDirectory),pathRow('接入配置文件',entry.configPath));
    if (entry.configExists === false) sheet.append(text('p','此配置文件尚未生成，可在工具初始化或接入后创建。','tool-installation-note'));
    if (locations(entry).length>1) {sheet.append(text('h3','检测到的安装'),installationPanel(entry));}
    return sheet;
  }
  function versionDisplay(value) { return value?.match(/\b\d+\.\d+\.\d+(?:\.\d+)*(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?/)?.[0] || value || '版本未知'; }
  function installationActions(entry, locked) {
    const group = text('div', '', 'tool-install-actions');
    const button = withIcon(actionButton('安装', 'tool-install', () => install(entry), locked || !entry.installation?.supported), 'down');
    if (!entry.installation?.supported) button.title = entry.installation?.reason || '暂无兼容安装方案';
    group.append(button);
    if (!entry.installation?.supported) group.append(actionButton('官方安装说明', 'quiet tool-download', async () => {
      try { await invoke('download_tool', {tool:entry.id}); } catch (error) { showResult('无法打开安装说明', String(error)); }
    }, locked));
    return group;
  }
  function versionCell(entry, locked) {
    const cell = text('div', '', `tool-install-state install-column${entry.installed ? ' installed' : ''}`);
    const copy = text('div', '', 'tool-version-copy'), update = updates.get(entry.id);
    if (update?.state === 'broken') {
      copy.append(text('span','无法运行','tool-version-empty'),text('small',update.note,'tool-location-error')); cell.append(copy); return cell;
    }
    if (!isPresent(entry) && !update?.canUpgrade) {
      copy.append(text('span', '未安装', 'tool-version-empty')); cell.append(copy, installationActions(entry, locked));
      if (!entry.installation?.supported) cell.append(text('span', entry.installation?.reason || '暂无兼容安装方案', 'tool-install-unavailable'));
      return cell;
    }
    const current = text('span', update ? versionDisplay(update.currentVersion) : checking ? '读取中…' : '已安装', 'tool-version'); current.title = update?.currentVersion || '正在读取版本'; copy.append(current);
    if ((update?.canUpgrade || update?.manualUpgrade) && (update.latestVersion || update.targetVersion)) copy.append(text('small', `新版本 ${versionDisplay(update.latestVersion || update.targetVersion)}`, 'tool-version-target'));
    else if (update?.state === 'unsupported_update') {copy.append(text('small', `新版本 ${versionDisplay(update.latestVersion)}`, 'tool-version-target'),text('small', '接入适配需更新', 'tool-update-badge'));}
    else if (update?.state === 'check_failed' || update?.state === 'unknown') copy.append(text('small', '版本检查未完成', 'tool-update-badge'));
    else if (update?.state === 'node_required') copy.append(text('small', update.note, 'tool-location-error'));
    if (update?.note) copy.title = update.note;
    cell.append(copy);
    if (update?.canUpgrade) {
      const button = withIcon(actionButton(entry.installed ? '升级' : '继续升级', 'tool-upgrade', () => confirmUpgrade(entry), locked));
      button.title = `升级至官方最新版 ${update.targetVersion}`; cell.append(button);
    } else if (update?.manualUpgrade) {
      const button = withIcon(actionButton('手动更新', 'tool-upgrade-guide', () => showManualUpdate(entry), locked), 'external'); button.title = '通过原安装渠道更新至官方最新版'; cell.append(button);
    }
    return cell;
  }
  async function runInstaller(entry, upgrade, current, onStatus, shouldStop = () => false, confirmedUpdate = null) {
    const update = confirmedUpdate || updates.get(entry.id);
    const targetVersion = upgrade ? update?.targetVersion : null;
    if (upgrade && !targetVersion) throw Error('更新信息已失效，请重新检查更新');
    let state = await invoke(upgrade ? 'start_tool_upgrade' : 'start_tool_installation', {tool:entry.id,...(upgrade ? {targetVersion,planRevision:update?.upgradeRevision || ''} : {})});
    let cancelSent = false;
    while (authenticated && current === generation) {
      onStatus(state);
      if (installTerminal(state)) return state;
      if (shouldStop() && !cancelSent) { await invoke('cancel_tool_installation', {tool:entry.id}); cancelSent = true; }
      await new Promise(resolve => setTimeout(resolve, 300));
      if (!authenticated || current !== generation) return null;
      state = await invoke('tool_installation_status', {tool:entry.id});
      if (!state) throw Error('安装状态暂时不可用，请检查更新后重试');
    }
    return null;
  }
  const installDialog = document.createElement('dialog');
  installDialog.className = 'tool-install-dialog';
  installDialog.innerHTML = '<h3></h3><p class="tool-install-status" role="status" aria-live="polite"></p><pre class="tool-install-output"></pre><div class="tool-actions"><button type="button" class="tool-install-cancel">取消安装</button><button type="button" class="tool-install-stop-wait" title="仅停止客户端等待，取消安装请在原生安装器中操作" hidden>停止等待</button><button type="button" class="tool-install-retry" hidden>重试安装</button><button type="button" class="tool-install-guide" hidden>查看其他安装方式</button><button type="button" class="tool-install-close" hidden>关闭</button></div>';
  document.body.append(installDialog);
  let installationTool = null, installationIsUpgrade = false, installationIsUninstall = false;
  const installTerminal = state => ['succeeded','failed','cancelled'].includes(state?.stage);
  const installStages = {preparing:'正在准备安装',downloading:'正在下载安装程序',dependencies:'正在准备运行依赖',installing:'正在安装',waiting_for_user:'请完成原生安装器中的步骤',detecting:'正在检测实际安装',succeeded:'安装完成',failed:'安装未完成',cancelled:'安装已取消'};
  function renderInstallation(state) {
    const operation = installationIsUninstall ? '卸载' : installationIsUpgrade ? '升级' : '安装';
    const label = (installStages[state.stage] || '正在安装').replaceAll('安装', operation);
    installDialog.querySelector('.tool-install-cancel').textContent = `取消${operation}`;
    installDialog.querySelector('.tool-install-retry').textContent = installationIsUninstall ? '重新选择安装' : `重试${operation}`;
    installDialog.querySelector('.tool-install-status').textContent = !state.detail ? label : state.detail.startsWith(label) ? state.detail : `${label}：${state.detail}`;
    const output = installDialog.querySelector('.tool-install-output'); output.textContent = (state.output || []).join('\n'); output.scrollTop = output.scrollHeight;
    const terminal = installTerminal(state);
    installDialog.querySelector('.tool-install-retry').disabled = installing;
    installDialog.querySelector('.tool-install-cancel').hidden = terminal || state.stage === 'waiting_for_user';
    installDialog.querySelector('.tool-install-stop-wait').hidden = state.stage !== 'waiting_for_user';
    installDialog.querySelector('.tool-install-retry').hidden = !['failed','cancelled'].includes(state.stage);
    installDialog.querySelector('.tool-install-guide').hidden = state.stage !== 'failed';
    installDialog.querySelector('.tool-install-close').hidden = !terminal && state.stage !== 'waiting_for_user';
  }
  async function install(entry, upgrade = false, confirmedUpdate = null) {
    if (!authenticated || installing || batching || checking || connecting || disconnecting || window.TokenMarketConnectionTests?.busy) return;
    const current = generation; installing = true; installationTool = entry; installationIsUpgrade = upgrade; installationIsUninstall = false;
    installDialog.querySelector('h3').textContent = `${upgrade ? '升级' : '安装'} ${entry.name}`;
    renderInstallation({stage:'preparing',detail:'安装不会自动接入或发起模型测试',output:[]});
    if (!installDialog.open) installDialog.showModal(); render();
    let operationSucceeded = false;
    try {
      const state = await runInstaller(entry, upgrade, current, renderInstallation, () => false, confirmedUpdate);
      operationSucceeded = state?.stage === 'succeeded';
      if (upgrade && operationSucceeded) updates.delete(entry.id);
    } catch (error) { if (authenticated && current === generation) renderInstallation({stage:'failed',detail:String(error),output:[]}); }
    finally {
      if (authenticated && current === generation) await refresh(true);
      installing = false; installDialog.querySelector('.tool-install-retry').disabled = false;
      if (authenticated) { render(); if (operationSucceeded || current !== generation) void checkUpdates(); }
    }
  }
  installDialog.querySelector('.tool-install-guide').onclick = async () => {
    if (!installationTool) return;
    try { await invoke('download_tool', {tool:installationTool.id}); } catch (error) { showResult('无法打开安装说明', String(error)); }
  };
  installDialog.querySelector('.tool-install-close').onclick = () => installDialog.close();
  installDialog.querySelector('.tool-install-retry').onclick = () => { if (installationTool) { if (installationIsUninstall) {installDialog.close(); void confirmUninstall(installationTool);} else void install(installationTool, installationIsUpgrade); } };
  installDialog.querySelector('.tool-install-cancel').onclick = async () => { if (!installationTool) return; try { await invoke('cancel_tool_installation', {tool:installationTool.id}); } catch (error) { feedback(String(error),true); } };
  installDialog.querySelector('.tool-install-stop-wait').onclick = installDialog.querySelector('.tool-install-cancel').onclick;
  installDialog.addEventListener('cancel', event => { if (installing) event.preventDefault(); });
  async function launch(entry) {
    const current = generation;
    try { await invoke('launch_connected_tool', {tool:entry.id}); if (authenticated && current === generation) feedback('已请求启动。现有会话请退出后重新启动以读取配置。'); }
    catch (error) { if (authenticated && current === generation) showResult('无法打开工具', String(error)); }
  }
  const uninstallDialog = document.createElement('dialog'); uninstallDialog.className = 'tool-uninstall-confirm'; document.body.append(uninstallDialog);
  async function confirmUninstall(entry) {
    if (!authenticated || installing || batching || checking || connecting || disconnecting || window.TokenMarketConnectionTests?.busy) return;
    const current = generation; installing = true; render();
    try {
      const candidates = await invoke('tool_uninstall_candidates', {tool:entry.id});
      if (!authenticated || current !== generation) return;
      uninstallDialog.replaceChildren(text('h3',`卸载 ${entry.name}`),text('p','选择要卸载的安装。只移除选定的程序，保留配置、会话、项目和接入记录。其他安装保留。'));
      const list = text('div','','tool-uninstall-candidates'), preview = text('pre','','tool-uninstall-command');
      let selected = candidates.length === 1 ? candidates[0].plan : null;
      const confirm = actionButton('确认卸载','danger tool-uninstall-execute',() => {
        if (!selected || !authenticated || current !== generation) return;
        uninstallDialog.close(); void uninstall(entry,selected);
      },!selected);
      candidates.forEach((candidate,index) => {
        const label = text('label','','tool-uninstall-candidate'), radio = document.createElement('input');
        radio.type = 'radio'; radio.name = 'uninstall-installation'; radio.value = String(index); radio.disabled = !candidate.plan; radio.checked = Boolean(selected && candidates.length === 1);
        radio.onchange = () => {selected = candidate.plan; preview.textContent = selected.command; confirm.disabled = false;};
        const info = text('div'); info.append(text('code',candidate.installation.path,'tool-path-value'));
        const metadata = text('div','','tool-uninstall-metadata');
        if (sourceLabel(candidate.installation.method)) metadata.append(text('span',sourceLabel(candidate.installation.method),'tool-install-method'));
        if (candidate.version) metadata.append(text('span',versionDisplay(candidate.version)));
        if (candidate.isPathDefault) metadata.append(text('span','终端默认'));
        info.append(metadata);
        if (candidate.error) info.append(text('p',candidate.error,'tool-location-error'));
        if (candidate.unavailableReason) info.append(text('p',candidate.unavailableReason,'tool-location-error'));
        if (candidate.reinstalledByUpdater) info.append(text('p','Grok 的更新器可能重新安装这份 npm 包，请检查其安装模式。','tool-location-error'));
        label.append(radio,info); list.append(label);
      });
      if (!candidates.length) list.append(text('p','未识别到可卸载的安装，请先检查更新。'));
      preview.textContent = selected?.command || '选择安装后显示卸载操作';
      const buttons = text('div','','tool-actions'); buttons.append(actionButton('取消','',()=>uninstallDialog.close()),actionButton('官方说明','quiet',async()=>{try{await invoke('download_tool',{tool:entry.id});}catch(e){feedback(String(e),true);}}),confirm);
      uninstallDialog.append(list,preview,buttons); uninstallDialog.showModal();
    } catch(error) {if (authenticated && current === generation) showResult('无法准备卸载',String(error));}
    finally {installing = false;if(authenticated)render();}
  }
  async function uninstall(entry,plan) {
    if (!authenticated || installing || batching || checking) return;
    const current = generation; installing = true; installationTool = entry; installationIsUninstall = true; installationIsUpgrade = false;
    installDialog.querySelector('h3').textContent = `卸载 ${entry.name}`; renderInstallation({stage:'preparing',detail:'配置和用户数据保留',output:[plan.command]}); installDialog.showModal(); render();
    try {
      let state = await invoke('start_tool_uninstall',{tool:entry.id,plan});
      while (authenticated && current === generation) {
        if (!state) throw Error('卸载状态不可用，请重新检查安装');
        renderInstallation(state); if (installTerminal(state)) break;
        await new Promise(resolve=>setTimeout(resolve,300));
        if (!authenticated || current !== generation) break;
        state = await invoke('tool_installation_status',{tool:entry.id});
      }
      updates.delete(entry.id);
    } catch(error) {if(authenticated && current===generation)renderInstallation({stage:'failed',detail:String(error),output:[]});}
    finally {if(authenticated && current===generation)await refresh(true);installing=false;installDialog.querySelector('.tool-install-retry').disabled=false;if(authenticated){render();void checkUpdates();}}
  }
  async function connect(entry) {
    const current = generation;
    try {
      if (entry.id === 'dumate') { await openDuMateModels(entry); return; }
      if (entry.connection) { await beginReconnect(entry); return; }
      await invoke('prepare_tool_connect', {tool:entry.id});
      if (!authenticated || current !== generation) return;
      const event = new CustomEvent('tokenmarket-connect-request', {detail:{tool:entry.id}, cancelable:true});
      if (window.dispatchEvent(event)) feedback('接入功能正在准备中。');
    } catch (error) { if (authenticated && current === generation) { showConnectFailure(); await refresh(); } }
  }
  function actions(entry, detailed = false) {
    const testing = Boolean(window.TokenMarketConnectionTests?.busy);
    const locked = loading || connecting || disconnecting || testing || installing || batching || checking;
    const group = text('div', '', 'tool-actions');
    const broken = updates.get(entry.id)?.state === 'broken';
    if (entry.connection) {
      group.append(actionButton('打开工具', 'primary tool-launch', () => launch(entry), !entry.installed || broken || locked));
      if (detailed) {
        const test = actionButton('测试连接', 'tool-test', () => window.dispatchEvent(new CustomEvent('tokenmarket-test-request', {detail:{id:entry.id,name:entry.name,connection:entry.connection}})), locked || testBlocked(entry));
        if (testBlocked(entry)) test.title = '请先检查配置或重新接入';
        group.append(test);
        const menu = text('details', '', 'tool-menu'); menu.dataset.liveKey = 'connection-menu'; menu.append(text('summary', '管理连接'));
        const options = text('div', '', 'tool-menu-options');
        options.append(actionButton('重新接入', 'tool-connect', event => { event.currentTarget.closest('details').open = false; void connect(entry); }, !entry.canConnect || locked));
        options.append(actionButton('取消接入', 'tool-disconnect', event => { event.currentTarget.closest('details').open = false; confirmDisconnectStart(entry); }, locked));
        menu.append(options); group.append(menu);
      }
    } else if (!entry.connection && entry.installed) {
      group.append(actionButton('接入', 'primary tool-connect', () => connect(entry), !entry.canConnect || broken || locked));
    }
    if (detailed && !entry.installed && !updates.get(entry.id)?.canUpgrade) {
      group.append(installationActions(entry, locked));
      if (!entry.installation?.supported) group.append(text('span', entry.installation?.reason || '暂无兼容安装方案', 'tool-install-unavailable'));
    }
    const update = updates.get(entry.id);
    if (detailed && update?.canUpgrade) {
      const button = actionButton(entry.installed ? '升级' : '继续升级', 'tool-upgrade', () => confirmUpgrade(entry), locked);
      button.title = `升级至官方最新版 ${update.targetVersion}`; group.append(button);
    }
    if (entry.installed || locations(entry).length) {
      const menu = text('details','','tool-lifecycle-menu'); menu.dataset.liveKey = 'lifecycle-menu'; menu.append(text('summary','更多'));
      menu.append(actionButton('卸载','danger tool-uninstall',event=>{event.currentTarget.closest('details').open=false;void confirmUninstall(entry);},locked)); group.append(menu);
    }
    return group;
  }
  function detailRow(label, value) { const row = text('div', '', 'info-row'); row.append(text('span', label), text('strong', value)); return row; }
  function showDetail(entry) {
    selectedTool = entry.id; renderDetail(entry); window.TokenMarketShell.show('detail', entry.name);
  }
  function showManualUpdate(entry) {
    showDetail(entry);
    const section = detailRoot.querySelector('.tool-manual-update');
    if (section) { section.tabIndex = -1; section.focus({preventScroll:true}); section.scrollIntoView({block:'center'}); }
  }
  function renderDetail(entry) {
    const nextDetail = document.createDocumentFragment();
    if (detailRoot.dataset.detailTool !== entry.id) { detailRoot.replaceChildren(); detailRoot.dataset.detailTool = entry.id; }
    const connection = entry.connection;
    nextDetail.append(actionButton('返回工具列表', 'quiet tool-detail-close', () => window.TokenMarketShell.show('tools')));
    const header = text('header', '', 'page-heading tool-detail-heading');
    const title = text('div', '', 'tool-detail-title'); const names = text('div'); names.append(text('h1', entry.name), text('p', entry.installed ? '已检测到本机安装' : '尚未安装')); title.append(toolIcon(entry), names);
    header.append(title); nextDetail.append(header); const operation = text('div', message.classList.contains('error') ? message.textContent : '', `tools-feedback tool-operation-status${message.classList.contains('error') ? ' error' : ''}`); operation.setAttribute('role', 'status'); nextDetail.append(operation);
    if (connection) {
      const notice = text('div', '', `notice ${testBlocked(entry) ? 'warning' : connection.state === 'test_failed' ? 'error' : connection.state === 'test_passed' ? 'success' : ''}`);
      notice.append(text('h3', connectionLabel(entry)));
      const note = connection.state === 'environment_mismatch' ? '当前工具使用其他环境的连接，尚未接入当前环境，测试已停用。请通过“管理连接 → 重新接入”切换到当前环境。' : connection.state === 'configuration_changed' ? '当前配置与接入记录不一致，测试已停用。请检查配置或重新接入。' : connection.state === 'test_passed' ? window.TokenMarketConnectionTests.scope : connection.state === 'test_failed' ? `未通过接口文本测试。请检查权限、余额与模型可用性后重试。${window.TokenMarketConnectionTests.scope}` : '配置已保存，尚未发起模型测试。可以先打开工具；需要时再确认费用并测试。';
      notice.append(text('p', note)); nextDetail.append(notice);
    } else nextDetail.append(text('p', entry.id === 'dumate-enterprise' ? '企业管理员需开放自定义模型与模型选择权限。先在 DuMate 登录企业版并完全退出，再接入；将使用当前企业登录态查询权限。导入后在此选择一个模型并确认费用测试，通过后重新打开 DuMate 并在会话中选择。' : entry.id === 'dumate' ? 'DuMate 个人版一次配置一个模型。接入、重新接入、切换模型或取消接入后，都需要完全退出并重新打开搭子后生效。' : entry.installed ? '工具已安装，接入后即可使用 Token Market 模型。' : '先安装工具，再返回检查更新和接入。', 'notice'));
    nextDetail.append(actions(entry, true));
    nextDetail.append(installationDetails(entry));
    const update = updates.get(entry.id);
    if (entry.installed && update) {
      const version = text('section', '', `sheet tool-version-sheet${update.manualUpgrade ? ' tool-manual-update' : ''}`);
      const target = update.latestVersion || update.targetVersion;
      const updateNote = update.manualUpgrade ? `${update.note}${target ? `。目标版本 ${versionDisplay(target)}` : ''}${entry.id === 'qwenwork' ? '。' : '，完成后返回点击“检查更新”。'}` : update.note;
      version.append(text('h2', update.manualUpgrade ? '手动更新' : '版本与更新'), detailRow('当前版本', versionDisplay(update.currentVersion)), detailRow('官方最新版本', update.latestVersion || (update.manualUpgrade && !target ? '请在原安装渠道查看' : '检查未完成')), text('p', updateNote));
      if (['check_failed','unknown','manual_update','unsupported_update'].includes(update.state) || !update.canUpgrade && update.currentVersion !== update.targetVersion) version.append(actionButton(update.manualUpgrade ? '打开官方更新指引' : '官方更新说明', 'quiet tool-download', async () => { try { await invoke('download_tool', {tool:entry.id}); } catch (error) { showResult('无法打开说明', String(error)); } }));
      nextDetail.append(version);
    }
    const grid = text('div', '', 'tool-detail-grid'), left = text('div');
    const models = text('section', '', 'sheet'); models.append(text('h2', '已配置模型'));
    if (connection?.models?.length) {
      connection.models.forEach(model => { const item = text('div', '', 'tool-model-entry'); item.append(text('code', model), text('span', '已配置', 'tool-model-tag')); models.append(item); });
    } else models.append(text('p', '接入后会显示该连接的可用模型。'));
    if (entry.id === 'dumate' && connection?.keyId) {
      models.append(actionButton('切换模型', 'tool-switch-model', () => openDuMateModels(entry, true),
        loading || connecting || disconnecting || installing || batching || checking || Boolean(window.TokenMarketConnectionTests?.busy) || testBlocked(entry)));
      models.append(text('p', '切换只保存所选模型，下次启动搭子时生效。若搭子正在运行，请重启后生效。'));
    }
    left.append(models);
    if (connection?.testResult) {
      const result = text('section', '', 'sheet'); result.append(text('h2', '最近一次接口文本测试'), detailRow('测试模型', connection.testResult.model), detailRow('测试时间', connection.testResult.testedAt), text('p', connection.testResult.detail), text('p', window.TokenMarketConnectionTests.scope, 'test-history-scope')); left.append(result);
    }
    const guidance = text('section', '', 'sheet'); guidance.append(text('h2', '使用前提醒'), text('p', entry.id === 'dumate' && connection ? '配置保存后，下次启动搭子时生效。若搭子正在运行，请重启；重启前会提示任务中断与定时任务调度的影响。' : '配置完成后，请完全退出并重新打开已有工具会话。'));
    if (entry.id === 'qwenwork') guidance.append(text('p', '接入或取消接入会保存配置，下次启动千问办公时生效。若千问办公正在运行，可等待当前任务完成后再重启；接入后在模型列表中选择 Token Market 模型。'));
    if (entry.id === 'openclaw') guidance.append(text('p', 'OpenClaw 需要重启 Gateway 或会话，已有模型白名单仍可能限制可选模型。'));
    if (entry.id === 'opencode') guidance.append(text('p', '项目或运行时配置可能覆盖全局模型设置。'));
    left.append(guidance);
    const context = text('section', '', 'sheet');
    if (connection) {
      context.append(text('h2', '连接归属'), detailRow('所属账号', connection.owner?.name || '未知账号'), detailRow('计费空间', connection.boundWorkspace?.name || '未知空间'), detailRow('安装状态', entry.installed ? '已安装' : '未安装'));
      context.append(text('p', '已有连接保留自己的计费空间。需要更换归属时，请确认重新接入。'));
    } else {
      context.append(text('h2', '工具信息'), detailRow('安装状态', entry.installed ? '已安装' : '未安装'), detailRow('接入状态', '未接入'));
      context.append(text('p', '接入后即可使用当前空间的模型。'));
    }
    grid.append(context, left); nextDetail.append(grid);
    for (const node of nextDetail.children) node.dataset.liveKey = [...node.classList].find(name => name.startsWith('tool-')) || node.classList[0] || node.tagName;
    window.TokenMarketLiveUI.children(detailRoot, nextDetail);
  }
  function render() {
    if (checkRequested && authenticated && !checking && !loading && !installing && !batching && !connecting && !disconnecting && !window.TokenMarketConnectionTests?.busy) queueMicrotask(() => void checkUpdates(checkExplicitRequested, !checkExplicitRequested));
    refreshButton.disabled = checking || installing || batching || connecting || disconnecting || Boolean(window.TokenMarketConnectionTests?.busy);
    refreshButton.textContent = checking ? '检查中…' : '检查更新';
    const upgradeCount = upgradeCandidates().length;
    upgradeAllButton.querySelector('span').textContent = `全部升级（${upgradeCount}）`;
    upgradeAllButton.disabled = !upgradeCount || refreshButton.disabled || loading;
    const nextList = document.createDocumentFragment();
    const visible = entries.filter(entry => matchesFilter(entry, filter) && `${entry.name} ${(entry.connection?.models || []).join(' ')}`.toLowerCase().includes(search));
    root.querySelectorAll('[data-tool-filter]').forEach(button => {
      const kind = button.dataset.toolFilter;
      const count = entries.filter(entry => matchesFilter(entry, kind)).length;
      const label = `${({all:'全部',connected:'已接入',available:'待接入',uninstalled:'未安装'})[kind]} ${count}`;
      if (button.textContent !== label) button.textContent = label;
      button.classList.toggle('selected', filter === kind); button.setAttribute('aria-pressed', String(filter === kind));
    });
    if (!entries.length) {
      const empty = text('div', '', 'tools-empty');
      empty.append(text('h2', loading ? '正在检测本机工具…' : message.classList.contains('error') ? '暂时无法检测本机工具' : '没有检测到工具'), text('p', loading ? '请稍候，安装状态即将更新。' : message.classList.contains('error') ? message.textContent : '安装工具后，点击检查更新。'));
      if (!loading) empty.append(actionButton('检查更新', '', () => refreshButton.click()));
      nextList.append(empty);
    } else if (!visible.length) {
      const empty = text('div', '', 'tools-empty'); empty.append(text('h2', '没有匹配的工具'), text('p', '试试其他名称，或查看全部工具。'), actionButton('查看全部工具', '', () => { search = ''; filter = 'all'; searchInput.value = ''; render(); })); nextList.append(empty);
    }
    const ordered = [...visible.filter(isPresent), ...visible.filter(entry => !isPresent(entry))];
    let previousGroup = null;
    ordered.forEach(entry => {
      const group = isPresent(entry) ? '已安装' : '未安装';
      if (!loading && group !== previousGroup) {const heading=text('h3',group,'tool-group-heading');heading.dataset.liveKey=group;nextList.append(heading);previousGroup=group;}
      const row = text('article', '', 'tool-row'); row.dataset.tool = entry.id;
      const name = actionButton('', 'tool-detail tool-name', () => showDetail(entry));
      name.setAttribute('aria-label', `查看 ${entry.name} 详情`);
      const title = text('span', '', 'tool-title');
      const heading = text('span','','tool-title-heading'); heading.append(text('strong',entry.name));
      const badge = sourceLabel(selectedLocation(entry)?.method);
      if (badge) heading.append(text('span',badge,'tool-install-method'));
      title.append(heading);
      if (entry.connection?.models?.length) title.append(text('span', entry.connection.models.length === 1 ? entry.connection.models[0] : `${entry.connection.models.length} 个已配置模型`, 'tool-model-summary'));
      if (entry.connection?.boundWorkspace?.name) title.append(text('span', `计费：${entry.connection.boundWorkspace.name}`, 'tool-bound-summary'));
      name.append(toolIcon(entry), title);
      const nameCell = text('div', '', 'tool-name-cell'); nameCell.append(name,installationSummary(entry));
      const install = versionCell(entry, loading || checking || installing || batching || connecting || disconnecting || Boolean(window.TokenMarketConnectionTests?.busy));
      const state = stateBadge(entry);
      const billing = text('span', entry.connection?.boundWorkspace?.name || '—', 'tool-billing billing-column');
      billing.title = entry.connection?.boundWorkspace?.name || '接入时使用当前空间';
      row.append(nameCell, install, state, billing, actions(entry));
      if (expandedInstallations.has(entry.id) && locations(entry).length>1) row.append(installationPanel(entry));
      nextList.append(row);
    });
    window.TokenMarketLiveUI.children(list, nextList);
    const countLabel = entries.length ? `显示 ${visible.length} 个工具，共 ${entries.length} 个` : '';
    if (root.querySelector('.tools-count').textContent !== countLabel) root.querySelector('.tools-count').textContent = countLabel;
    const selected = entries.find(entry => entry.id === selectedTool);
    if (selected) renderDetail(selected);
  }
  async function refresh(waitForCurrent = false) {
    if (!authenticated) return;
    if (loading) { if (waitForCurrent === true) { await inventoryPending; return refresh(true); } return; }
    let finishInventory;
    inventoryPending = new Promise(resolve => { finishInventory = resolve; });
    const firstLoad = entries.length === 0;
    let changed = false;
    loading = true; const current = generation;
    // Retain the current DOM during background detection: loading placeholders
    // and rebuilding unchanged rows collapse menus and move the page content.
    if (firstLoad) render();
    let success = false;
    try {
      const next = (await invoke('desktop_tool_inventory')).filter(entry => isVisibleTool(entry.id));
      if (authenticated && current === generation) {
        changed = JSON.stringify(entries) !== JSON.stringify(next);
        if (changed) entries = next;
        success = true;
        if (message.classList.contains('error')) feedback('');
      }
    } catch (error) { if (authenticated && current === generation) { feedback(String(error), true); } }
    finally {
      loading = false; finishInventory();
      if (authenticated && current === generation) { if (firstLoad || changed) render(); }
      else if (authenticated) refresh();
    }
    return success;
  }
  const disconnectConfirm = document.createElement('dialog');
  disconnectConfirm.className = 'disconnect-confirm-dialog';
  document.body.append(disconnectConfirm);
  function confirmDisconnectStart(entry) {
    disconnectConfirm.replaceChildren(text('h3', '确认取消接入'), text('p', `将恢复 ${entry.name} 本次连接管理的配置，并停止该工具的接入。计费空间：${entry.connection.boundWorkspace.name}。`));
    const buttons = text('div', '', 'tool-actions');
    buttons.append(actionButton('保留连接', '', () => disconnectConfirm.close()), actionButton('确认取消接入', 'danger', () => { disconnectConfirm.close(); runDisconnect(entry, {revision:null,resolutions:[]}); }));
    disconnectConfirm.append(buttons); disconnectConfirm.showModal();
  }
  window.addEventListener('current-workspace-changed', () => { if (disconnectConfirm.open) disconnectConfirm.close(); });
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
        confirmDisconnect.textContent = '确认恢复并取消接入';
        disconnectDialog.querySelector('.disconnect-cancel').textContent = '暂不取消接入';
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
        if (entry.id === 'dumate') {
          const {notice,canRestart}=await duMateSavedStatus();
          if (!authenticated || current !== generation) return;
          feedback(notice);showResult('取消接入配置已保存',notice,canRestart);
        } else if (entry.id === 'qwenwork') {
          const {notice,canRestart}=await qwenWorkSavedStatus();
          if (!authenticated || current !== generation) return;
          feedback(notice);showResult('取消接入配置已保存',notice,false,canRestart);
        } else {
          const detail=`${entry.name} 已取消接入。`;feedback(detail);showResult('取消接入完成',detail);
        }
      }
    } catch (error) { if (authenticated && current === generation) { feedback(String(error), true); showResult('取消接入未完成', String(error)); } }
    finally { disconnecting = false; if (authenticated && current === generation) render(); }
  }
  const reconnectDialog = document.createElement('dialog'); reconnectDialog.className = 'reconnect-dialog';
  reconnectDialog.innerHTML = '<h3>确认重新接入</h3><p class="reconnect-from"></p><p class="reconnect-to"></p><p>将使用当前账号与空间重新接入此工具。</p><div class="tool-actions"><button class="reconnect-cancel">取消</button><button class="primary reconnect-confirm">确认重新接入</button></div>';
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
    if (!context?.owner_user_id || !context.current_workspace?.id) { showConnectFailure(); return; }
    connectDialog.querySelector('h3').textContent = `接入 ${supportedConnections[tool]}`;
    pendingConnect = {tool,ownerId:context.owner_user_id,workspaceId:context.current_workspace.id};
    connectDialog.querySelector('.connect-target').textContent = `账号：${context.user.display_name || context.user.user_name} · 计费空间：${context.current_workspace.name}`;
    connectDialog.showModal();
  }
  connectDialog.querySelector('.connect-cancel').onclick = () => { pendingConnect = null; connectDialog.close(); };
  connectDialog.addEventListener('cancel', () => { pendingConnect = null; });
  connectDialog.querySelector('.connect-confirm').onclick = () => {
    const pending = pendingConnect; pendingConnect = null; connectDialog.close();
    const context = window.TokenMarketWorkspace.current;
    if (!pending || !context) return;
    if (context.owner_user_id !== pending.ownerId || context.current_workspace.id !== pending.workspaceId) { showResult('账号或空间已改变', '请重新确认接入归属。'); return; }
    runConnect(pending.tool);
  };
  window.addEventListener('current-workspace-changed', () => { pendingConnect = null; if (connectDialog.open) connectDialog.close(); });
  const dumateDialog = document.createElement('dialog'); dumateDialog.className = 'dumate-model-dialog';
  dumateDialog.innerHTML = '<h3></h3><p class="dumate-model-target"></p><p class="dumate-model-note">配置在下次启动搭子时生效。若搭子正在运行，请重启后生效。保存不会自动退出搭子，也不会发起模型测试。</p><label>选择模型 <select class="dumate-model-select" aria-label="搭子使用的模型"></select></label><p class="dumate-model-status" role="status" aria-live="polite"></p><div class="tool-actions"><button class="dumate-model-cancel">取消</button><button class="dumate-model-retry">重新加载</button><button class="primary dumate-model-confirm">确认接入</button></div>';
  document.body.append(dumateDialog);
  let pendingDuMate = null;
  const dumateSelect = dumateDialog.querySelector('select');
  const dumateConfirm = dumateDialog.querySelector('.dumate-model-confirm');
  const dumateRetry = dumateDialog.querySelector('.dumate-model-retry');
  function closeDuMateModels() { pendingDuMate = null; if (dumateDialog.open) dumateDialog.close(); }
  dumateDialog.querySelector('.dumate-model-cancel').onclick = closeDuMateModels;
  dumateDialog.addEventListener('cancel', event => { if (pendingDuMate?.saving) event.preventDefault(); else pendingDuMate = null; });
  window.addEventListener('current-workspace-changed', closeDuMateModels);
  async function openDuMateModels(entry, preserveBinding = false) {
    if (restartingDuMate) {feedback('搭子正在重启，请稍后再修改配置。',true);return;}
    if (!authenticated || connecting || disconnecting || window.TokenMarketConnectionTests?.busy) return;
    const current = generation;
    await window.TokenMarketWorkspace.refresh();
    if (!authenticated || current !== generation) return;
    const context = window.TokenMarketWorkspace.current;
    if (!context?.owner_user_id || !context.current_workspace?.id) { showResult('无法确认当前账号', '请刷新账号与空间后重新选择模型。'); return; }
    const pending = {entry,preserveBinding,generation:current,target:{expectedKeyId:entry.connection?.keyId || null,
      expectedOwnerId:context.owner_user_id,expectedWorkspaceId:context.current_workspace.id,preserveBinding,recovery:null}};
    pendingDuMate = pending;
    dumateDialog.querySelector('h3').textContent = preserveBinding ? '切换搭子模型' : entry.connection ? '重新接入百度搭子' : '接入百度搭子';
    dumateDialog.querySelector('.dumate-model-note').textContent = `${preserveBinding ? '保留当前密钥和计费空间。切换' : '接入'}只保存配置，下次启动搭子时生效。若搭子正在运行，请重启后生效。不会自动发起模型测试。`;
    dumateConfirm.textContent = preserveBinding ? '确认切换模型' : entry.connection ? '确认重新接入' : '确认接入';
    const owner = preserveBinding ? entry.connection.owner?.name : context.user.display_name || context.user.user_name;
    const space = preserveBinding ? entry.connection.boundWorkspace?.name : context.current_workspace.name;
    dumateDialog.querySelector('.dumate-model-target').textContent = `账号：${owner} · 计费空间：${space}`;
    dumateDialog.showModal();
    await loadDuMateModels(pending);
  }
  function duMateModelError(error, pending, resume) {
    if (error?.recovery) {
      dumateDialog.close();
      pendingRecovery = {tool:'dumate',target:error.recovery,generation:pending.generation,resume:() => {
        if (pendingDuMate !== pending) return;
        pending.target.recovery = error.recovery; dumateDialog.showModal(); void resume();
      }};
      recoveryDialog.querySelector('.recovery-scope').textContent = `账号：${error.recovery.ownerName} · 计费空间：${error.recovery.workspaceName}`;
      recoveryDialog.showModal();
    } else dumateDialog.querySelector('.dumate-model-status').textContent = error?.message || String(error);
  }
  async function loadDuMateModels(pending) {
    if (connecting || !authenticated || pendingDuMate !== pending) return;
    connecting = true; render(); dumateSelect.replaceChildren(); dumateSelect.disabled = true;
    dumateConfirm.disabled = true; dumateRetry.hidden = true;
    dumateDialog.querySelector('.dumate-model-status').textContent = '正在加载可用模型…';
    try {
      const result = await invoke('prepare_dumate_models', {target:pending.target});
      if (!authenticated || pending.generation !== generation || pendingDuMate !== pending) return;
      result.models.forEach(model => { const option = text('option', model); option.value = model; dumateSelect.append(option); });
      const preferred = pending.entry.connection?.models?.[0];
      dumateSelect.value = result.models.includes(preferred) ? preferred : result.selectedModel;
      dumateSelect.disabled = false; dumateConfirm.disabled = !dumateSelect.value;
      dumateDialog.querySelector('.dumate-model-status').textContent = result.models.length ? `可选 ${result.models.length} 个模型` : '当前连接没有可用模型，请稍后重新加载。';
    } catch (error) {
      if (authenticated && pending.generation === generation && pendingDuMate === pending) duMateModelError(error, pending, () => loadDuMateModels(pending));
    } finally {
      connecting = false;
      if (authenticated && pendingDuMate === pending) { dumateRetry.hidden = false; render(); }
    }
  }
  dumateRetry.onclick = () => { if (pendingDuMate) void loadDuMateModels(pendingDuMate); };
  async function saveDuMateModel(pending, model) {
    if (!authenticated || restartingDuMate || connecting || disconnecting || pendingDuMate !== pending || !model) return;
    connecting = true; pending.saving = true; render(); dumateConfirm.disabled = true; dumateRetry.disabled = true;
    dumateSelect.disabled = true; dumateDialog.querySelector('.dumate-model-cancel').disabled = true;
    dumateDialog.querySelector('.dumate-model-status').textContent = '正在保存所选模型…';
    try {
      const outcome = await invoke('configure_dumate_model', {target:pending.target,model});
      if (!authenticated || pending.generation !== generation || pendingDuMate !== pending) return;
      closeDuMateModels(); await refresh(true);
      if (!authenticated || pending.generation !== generation) return;
      const {notice,canRestart}=await duMateSavedStatus();
      if (!authenticated || pending.generation !== generation) return;
      launchTool='dumate';
      dialog.querySelector('h3').textContent=pending.preserveBinding ? '模型切换配置已保存' : pending.entry.connection ? '重新接入配置已保存' : '接入配置已保存';
      dialog.querySelector('.restart-notice').textContent=`所选模型：${model}。${notice}`;
      dialog.querySelector('.launch').hidden=true;
      dialog.querySelector('.restart-dumate').hidden=!canRestart;
      dialog.querySelector('.restart-qwenwork').hidden=true;
      dialog.querySelector('.close').textContent=canRestart ? '稍后手动重启搭子' : '完成';
      dialog.querySelector('.launch-error')._restartToken=null;dialog.querySelector('.launch-error').textContent='';dialog.showModal();

    } catch (error) {
      if (authenticated && pending.generation === generation && pendingDuMate === pending) duMateModelError(error, pending, () => saveDuMateModel(pending, model));
    } finally {
      connecting = false; pending.saving = false; dumateRetry.disabled = false; dumateSelect.disabled = false;
      dumateDialog.querySelector('.dumate-model-cancel').disabled = false; dumateConfirm.disabled = !dumateSelect.value;
      if (authenticated && pending.generation === generation) render();
    }
  }
  dumateConfirm.onclick = () => { if (pendingDuMate) void saveDuMateModel(pendingDuMate, dumateSelect.value); };
  const supportedConnections = {'claude-code':'Claude Code', codex:'Codex', 'deepseek-harness':'DeepSeek Harness',workbuddy:'WorkBuddy',openclaw:'OpenClaw',opencode:'OpenCode','grok-build':'Grok Build',hermes:'Hermes',pi:'Pi','minimax-code':'MiniMax Code','oh-my-pi':'Oh My Pi','kimi-code':'Kimi Code',dumate:'百度搭子 DuMate（个人版）','dumate-enterprise':'百度搭子 DuMate（企业版）',qwenwork:'千问办公'};
  let launchTool = 'claude-code';
  const dialog = document.createElement('dialog');
  dialog.className = 'tool-connect-result-dialog';
  dialog.innerHTML = '<h3></h3><p class="restart-notice"></p><p class="launch-error error" role="alert"></p><form method="dialog" class="tool-actions"><button class="primary restart-dumate" type="button" hidden>重启搭子</button><button class="primary restart-qwenwork" type="button" hidden>重启千问办公</button><button class="primary launch" type="button">打开终端运行 Claude Code</button><button class="close" type="submit">完成</button></form>';
  document.body.append(dialog);
  document.body.append(disconnectDialog);
  dialog.addEventListener('close',()=>{
    const status=dialog.querySelector('.launch-error');
    if (status._restartToken) { status._restartToken=null;closeRestartConfirmation(); }
  });
  dialog.querySelector('.restart-dumate').onclick=()=>restartDuMate(dialog.querySelector('.restart-dumate'),dialog.querySelector('.launch-error'));
  dialog.querySelector('.restart-qwenwork').onclick=()=>restartQwenWork(dialog.querySelector('.restart-qwenwork'),dialog.querySelector('.launch-error'));
  dialog.querySelector('.launch').onclick = async () => {
    try { await invoke('launch_connected_tool', {tool:launchTool}); dialog.close(); }
    catch(error) { dialog.querySelector('.launch-error').textContent = '打开失败，请联系管理员。'; }
  };
  window.addEventListener('tokenmarket-connect-request', event => {
    const tool = event.detail.tool;
    if (!supportedConnections[tool]) return;
    if (tool === 'dumate') { event.preventDefault(); const entry = entries.find(item => item.id === tool); if (entry) void openDuMateModels(entry); return; }
    event.preventDefault(); confirmInitialConnect(tool).catch(() => { if (authenticated) showConnectFailure(); });
  });
  const recoveryDialog = document.createElement('dialog'); recoveryDialog.className = 'recovery-dialog';
  recoveryDialog.innerHTML = '<h3>确认恢复连接</h3><p>本机保存的连接信息已丢失。确认后将重新建立此工具的连接，并在接入成功后替换对应的旧凭证。</p><p class="recovery-scope"></p><p>其他账号、空间和工具的连接不受影响。</p><div class="tool-actions"><button class="recovery-cancel">暂不恢复</button><button class="primary recovery-confirm">确认恢复并接入</button></div>';
  document.body.append(recoveryDialog);
  recoveryDialog.querySelector('.recovery-cancel').onclick = () => { if (pendingRecovery?.resume) closeDuMateModels(); pendingRecovery = null; recoveryDialog.close(); };
  recoveryDialog.addEventListener('cancel', () => { if (pendingRecovery?.resume) closeDuMateModels(); pendingRecovery = null; });
  recoveryDialog.querySelector('.recovery-confirm').onclick = () => {
    const pending = pendingRecovery; pendingRecovery = null; recoveryDialog.close();
    if (pending && authenticated && pending.generation === generation) { if (pending.resume) pending.resume(); else void runConnect(pending.tool, pending.replacement, pending.target); }
  };
  window.addEventListener('current-workspace-changed', () => { pendingRecovery = null; if (recoveryDialog.open) recoveryDialog.close(); });
  async function runConnect(tool, replacement = null, recovery = null) {
    const name = supportedConnections[tool];
    if (!name || !authenticated || connecting || disconnecting || window.TokenMarketConnectionTests?.busy) return;
    if (!replacement && entries.find(entry => entry.id === tool)?.connection) {
      feedback('连接已改变，请刷新后使用重新接入。', true); return;
    }
    const actionGeneration = generation;
    connecting = true; render(); feedback(`正在接入 ${name}，请稍候…`);
    try {
      await (replacement
        ? invoke('reconnect_tool', {tool,expectedKeyId:replacement.entry.connection.keyId,expectedOwnerId:replacement.preview.currentOwner.id,expectedWorkspaceId:replacement.preview.currentWorkspace.id,recovery})
        : invoke('connect_tool', {tool,recovery}));
      if (!authenticated || actionGeneration !== generation) return;
      await refresh(true);
      if (!authenticated || actionGeneration !== generation) return;
      feedback('');
      const qwenworkStatus=tool==='qwenwork' ? await qwenWorkSavedStatus() : null;
      if (!authenticated || actionGeneration !== generation) return;
      launchTool = tool;
      dialog.querySelector('.launch-error')._restartToken=null;
      dialog.querySelector('.restart-dumate').hidden=true;
      dialog.querySelector('.restart-qwenwork').hidden=!qwenworkStatus?.canRestart;
      dialog.querySelector('.launch').hidden=false;
      dialog.querySelector('.close').textContent='完成';
      dialog.querySelector('h3').textContent = `${name} 已接入`;
      if (tool==='qwenwork') {
        dialog.querySelector('h3').textContent=replacement ? '重新接入配置已保存' : '接入配置已保存';
        dialog.querySelector('.close').textContent=qwenworkStatus.canRestart ? '稍后手动重启千问办公' : '完成';
      }
      const guidance = {
        'deepseek-harness':'请重新打开所选 profile。',
        qwenwork:qwenworkStatus ? `${qwenworkStatus.notice}打开后，在模型列表中选择 Token Market 模型。` : '',
        workbuddy:'请完全退出并重新打开 WorkBuddy，在模型列表中选择 Token Market 模型。',
        openclaw:'请重启 OpenClaw Gateway 并重新打开会话。',
        opencode:'请重新打开 OpenCode，在模型列表中选择 Token Market 模型。',
        'dumate-enterprise':'请先在工具详情中测试所选模型，通过后重新打开 DuMate 并选择该模型。',
        dumate:'请完全退出并重新打开 DuMate。',
        'kimi-code':'请在模型列表中选择 Token Market 模型。',
        'oh-my-pi':'请通过 /model 选择 Token Market 模型。',
        pi:'请通过 /model 选择 Token Market 模型。',
        'minimax-code':'请重新启动 MiniMax Code，通过 /model 选择 Token Market 模型。',
        'grok-build':'请重新启动 Grok Build，通过 /model 选择 Token Market 模型。',
        hermes:'请重新启动 Hermes，通过 /model 选择 Token Market 模型。',
      };
      dialog.querySelector('.restart-notice').textContent = guidance[tool] || `请重新启动 ${name}。`;
      dialog.querySelector('.launch').hidden = tool==='qwenwork';
      dialog.querySelector('.launch').textContent = ['deepseek-harness','workbuddy','dumate','dumate-enterprise','qwenwork'].includes(tool) ? `打开 ${name}` : `打开工具 / 终端运行 ${name}`;
      dialog.querySelector('.launch-error').textContent = ''; dialog.showModal();
    } catch(error) {
      if (authenticated && actionGeneration === generation) {
        if (error && typeof error === 'object' && error.recovery) {
          pendingRecovery = {tool,replacement,target:error.recovery,generation:actionGeneration};
          recoveryDialog.querySelector('.recovery-scope').textContent = `账号：${error.recovery.ownerName} · 计费空间：${error.recovery.workspaceName}`;
          recoveryDialog.showModal(); feedback('请确认是否恢复这条连接。');
        } else {
          showConnectFailure();
        }
      }
    }
    finally { connecting = false; if (authenticated && actionGeneration === generation) render(); }
  }
  window.addEventListener('tokenmarket-test-busy', render);
  const upgradeConfirm = document.createElement('dialog'); upgradeConfirm.className = 'tool-upgrade-confirm';
  document.body.append(upgradeConfirm);
  function confirmUpgrade(entry) {
    const update = updates.get(entry.id); if (!update?.canUpgrade || installing || batching || checking) return;
    upgradeConfirm.replaceChildren(text('h3', `升级 ${entry.name}`), text('p', `${update.currentVersion} → ${update.targetVersion}`, 'upgrade-version'), text('p', '将升级到当前检查到的官方最新版。请先完全退出该工具；升级后重新打开，已有连接无需重新接入。'));
    if (locations(entry).length > 1) upgradeConfirm.append(installationPanel(entry));
    if (update.upgradePath) upgradeConfirm.append(pathRow('本次升级入口',update.upgradePath));
    if (update.upgradeCommand) upgradeConfirm.append(text('pre',update.upgradeCommand,'tool-upgrade-command'));
    const buttons = text('div', '', 'tool-actions');
    buttons.append(actionButton('取消', '', () => upgradeConfirm.close()), actionButton('确认升级', 'primary', () => { upgradeConfirm.close(); void install(entry, true, update); }));
    upgradeConfirm.append(buttons); upgradeConfirm.showModal();
  }
  const batchDialog = document.createElement('dialog'); batchDialog.className = 'tool-batch-upgrade-dialog'; document.body.append(batchDialog);
  batchDialog.addEventListener('cancel', event => { if (batching) event.preventDefault(); });
  function upgradeCandidates() {
    const seen = new Set();
    return entries.filter(entry => {
      const installation = entry.id === 'dumate-enterprise' ? 'dumate' : entry.id;
      if (!entry.installed || !updates.get(entry.id)?.canUpgrade || seen.has(installation)) return false;
      seen.add(installation); return true;
    });
  }
  function confirmUpgradeAll() {
    if (!authenticated || checking || installing || batching || connecting || disconnecting || window.TokenMarketConnectionTests?.busy) return;
    const candidates = upgradeCandidates().map(entry => ({entry, update:{...updates.get(entry.id)}}));
    if (!candidates.length) return;
    batchDialog.replaceChildren(text('h3', `全部升级（${candidates.length}）`), text('p', '将依次升级以下工具至官方最新版。请先完全退出这些工具；已有连接保持不变。需要手动更新或接入适配尚未支持的工具不会加入队列。'));
    const queue = text('div', '', 'batch-upgrade-queue');
    const jobs = candidates.map(({entry,update}) => {
      const row = text('div', '', 'batch-upgrade-row'), info = text('div');
      info.append(text('strong', entry.name), text('small', `${versionDisplay(update.currentVersion)} → ${versionDisplay(update.targetVersion)}`));
      const status = text('span', '等待升级', 'batch-tool-status'); row.append(info, status); queue.append(row);
      return {entry, update, status};
    });
    const summary = text('p', '', 'batch-upgrade-summary'); summary.setAttribute('role','status'); summary.setAttribute('aria-live','polite');
    const controls = text('div', '', 'tool-actions');
    const cancel = actionButton('取消', 'batch-upgrade-cancel', () => batchDialog.close());
    const start = actionButton('确认全部升级', 'primary batch-upgrade-start', () => void runBatch(jobs, summary, controls));
    controls.append(cancel,start); batchDialog.append(queue,summary,controls); batchDialog.showModal();
  }
  async function runBatch(jobs, summary, controls) {
    if (!authenticated || installing || batching || checking) return;
    const current = generation; batching = true; batchStop = false; render();
    const stop = actionButton('停止全部升级', 'batch-upgrade-stop', () => {batchStop = true; stop.disabled = true; summary.textContent = '正在停止当前操作，剩余工具将不再升级；原生安装器需在其窗口中取消。';});
    controls.replaceChildren(stop);
    let succeeded = 0, failed = 0, skipped = 0;
    try {
      for (const [index,job] of jobs.entries()) {
        if (!authenticated || current !== generation || batchStop) { skipped = jobs.length-index; break; }
        summary.textContent = `正在升级 ${index+1}/${jobs.length}：${job.entry.name}`;
        try {
          const state = await runInstaller(job.entry, true, current, state => {
            job.status.textContent = (installStages[state.stage] || '升级中').replaceAll('安装','升级'); job.status.title = state.detail || '';
          }, () => batchStop, job.update);
          if (!state) { skipped = jobs.length-index-1; break; }
          if (state.stage === 'succeeded') { succeeded++; updates.delete(job.entry.id); }
          else { failed++; job.status.textContent = state.stage === 'cancelled' ? '已停止' : `升级失败：${state.detail || '请单独重试'}`; }
        } catch(error) { failed++; skipped = jobs.length-index-1; job.status.textContent = `升级状态未确认：${String(error)}`; job.status.title = String(error); break; }
      }
    } finally {
      batching = false;
      if (authenticated && current === generation) {
        jobs.filter(job=>job.status.textContent==='等待升级').forEach(job=>{job.status.textContent='未执行';});
        summary.textContent = `已升级 ${succeeded} 个 · 未完成 ${failed} 个${skipped ? ` · 未执行 ${skipped} 个` : ''}`;
        controls.replaceChildren(actionButton('完成', 'primary batch-upgrade-close', () => batchDialog.close()));
        await refresh(true); void checkUpdates();
      } else if (authenticated) { render(); void checkUpdates(); }
    }
  }
  withIcon(upgradeAllButton); upgradeAllButton.onclick = confirmUpgradeAll;
  function checkUpdates(explicit = false, silent = false) {
    if (!authenticated) return Promise.resolve();
    if (updatePending) return updatePending;
    if (installing || batching || connecting || disconnecting || window.TokenMarketConnectionTests?.busy) {
      checkRequested = true; checkExplicitRequested ||= explicit;
      if (explicit) updateStatus.textContent = '当前操作完成后将检查更新';
      return Promise.resolve();
    }
    checkRequested = false; checkExplicitRequested = false;
    const current = generation; checking = !silent;
    if (!silent) { render(); updateStatus.textContent = '正在检测本机工具与官方版本…'; }
    const pending = (async () => {
      try {
        const success = await refresh(true);
        if (!authenticated || current !== generation) return;
        if (!success) throw Error('本机工具检测失败，请稍后重试');
        const result = (await invoke('check_tool_updates')).filter(update => isVisibleTool(update.tool));
        if (!authenticated || current !== generation) return;
        updates = new Map(result.map(update => [update.tool, update]));
        const installed = result.filter(update => update.state !== 'not_installed');
        const available = installed.filter(update => update.canUpgrade).length;
        const manual = installed.filter(update => update.manualUpgrade).length;
        const restricted = installed.filter(update => update.state === 'unsupported_update').length;
        const failed = installed.filter(update => ['check_failed','unknown','busy','broken','node_required'].includes(update.state)).length;
        updateStatus.textContent = `已检查 ${installed.length} 个已安装工具 · ${available} 个可升级${manual ? ` · ${manual} 个需手动更新` : ''}${restricted ? ` · ${restricted} 个接入适配需更新` : ''}${failed ? ` · ${failed} 个检查未完成，可重试` : ''} · ${new Date().toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})}`;
      } catch (error) {
        if (!silent && authenticated && current === generation) { updates.clear(); updateStatus.textContent = `检查未完成：${String(error)}`; }
      } finally {
        if (current === generation) { checking = false; render(); }
        if (updatePending === pending) updatePending = null;
      }
    })();
    updatePending = pending; return pending;
  }
  refreshButton.onclick = () => void checkUpdates(true);
  window.__TAURI__.event.listen('desktop-check-tool-updates', () => void checkUpdates()).catch(error => console.error(error));

  window.TokenMarketTools = {
    refresh,
    sync: () => checkUpdates(false, true),
    setAuthenticated(value) {
      value = Boolean(value);
      root.hidden = !value;
      if (value === authenticated) return;
      authenticated = value; generation += 1;
      closeRestartConfirmation();
      if (!value && installDialog.open) installDialog.close();
      if (uninstallDialog.open) uninstallDialog.close();
      window.dispatchEvent(new CustomEvent('tokenmarket-tools-auth', {detail:{authenticated:value}}));
      checking = false; updatePending = null; checkRequested = false; updates.clear(); expandedInstallations.clear(); updateStatus.textContent = '';
      if (upgradeConfirm.open) upgradeConfirm.close();
      batchStop = true; if (batchDialog.open) batchDialog.close();
      if (value) void checkUpdates();
      else { closeDuMateModels(); pendingRecovery = null; if (recoveryDialog.open) recoveryDialog.close(); if (disconnectConfirm.open) disconnectConfirm.close(); if (resultDialog.open) resultDialog.close(); if (reconnectDialog.open) reconnectDialog.close(); pendingReconnect = null; if (dialog.open) dialog.close(); if (disconnectDialog.open) disconnectDialog.close(); pendingDisconnect = null; entries = []; selectedTool = null; detailRoot.replaceChildren(); pendingConnect = null; if (connectDialog.open) connectDialog.close(); list.replaceChildren(); feedback(''); }
    },
  };
})();
