/* Client maintenance stays available without a Client Session or workspace. */
(() => {
  if (!window.__TAURI__) return;
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const publish = (value, action) => document.dispatchEvent(new CustomEvent('client-update-action-result', {detail:{value,action}}));
  const el = id => document.getElementById(id);
  const progress = document.createElement('div');
  progress.className = 'tool-download-progress';
  progress.hidden = true;
  progress.innerHTML = '<progress max="100" aria-label="客户端更新下载进度"></progress><p class="tool-download-summary"></p>';
  el('client-update-status').after(progress);
  const check = el('check-client-update');
  const recovery = el('client-update-recovery');
  recovery.onclick = async () => {
    recovery.disabled = true;
    try { await invoke('open_client_update_recovery'); }
    catch (error) {
      el('client-recovery-help').textContent = String(error);
      el('client-recovery-help').hidden = false;
    }
    finally { recovery.disabled = !latest?.recoveryAvailable; }
  };
  let checking = false;
  let currentVersion = '未知';
  let latest = null;
  let downloading = false;
  const download = el('download-client-update');
  const confirm = document.createElement('dialog');
  confirm.setAttribute('aria-label', '确认下载客户端更新');
  confirm.innerHTML = '<h3>确认下载客户端更新</h3><p class="client-download-version"></p><p>下载后将验证更新包签名。安装和重启需要另行确认。</p><div class="tool-actions"><button class="cancel-download">取消下载</button><button class="primary confirm-download">确认下载</button></div>';
  document.body.append(confirm);
  let confirmedVersion = null;
  confirm.querySelector('.cancel-download').onclick = () => confirm.close();
  download.onclick = () => {
    if (checking || downloading || !latest?.availableVersion) return;
    confirmedVersion = latest.availableVersion;
    confirm.querySelector('.client-download-version').textContent = `Token Market ${confirmedVersion}`;
    confirm.showModal();
  };
  confirm.querySelector('.confirm-download').onclick = async () => {
    if (downloading) return;
    const version = confirmedVersion;
    confirm.close();
    document.dispatchEvent(new Event('client-update-manual-check'));
    downloading = true;
    publish({...latest,status:'downloading',downloadProgress:null,error:null}, 'download');
    try { publish(await invoke('download_client_update', {version}), 'download'); }
    catch (error) { publish({...latest,status:'failed',currentVersion,error:String(error)}, 'download'); }
    finally { downloading = false; }
  };
  const install = el('install-client-update');
  let installing = false;
  const installConfirm = document.createElement('dialog');
  installConfirm.setAttribute('aria-label', '确认安装并重启');
  installConfirm.innerHTML = '<h3>确认安装并重启</h3><p class="client-install-version"></p><p>安装前将检查桌面和 CLI 的活动操作。请先保存工作；安装交接后应用将关闭并重启。</p><div class="tool-actions"><button class="cancel-install">取消安装</button><button class="primary confirm-install">确认安装并重启</button></div>';
  document.body.append(installConfirm);
  let installVersion;
  installConfirm.querySelector('.cancel-install').onclick = () => installConfirm.close();
  install.onclick = () => {
    if (installing || latest?.status !== 'ready' || !latest.installAvailable) return;
    installVersion = latest.availableVersion;
    installConfirm.querySelector('.client-install-version').textContent = `Token Market ${installVersion}`;
    installConfirm.showModal();
  };
  installConfirm.querySelector('.confirm-install').onclick = async () => {
    if (installing) return;
    installConfirm.close();
    installing = true;
    document.dispatchEvent(new Event('client-update-manual-check'));
    publish({...latest,status:'installing',error:null}, 'install');
    try { publish(await invoke('install_client_update', {version:installVersion}), 'install-result'); }
    catch (error) { publish({...latest,status:'ready',error:String(error)}, 'install-error'); }
    finally { installing = false; render(latest); }
  };
  function render(value) {
    if (value.automatic && !['available','downloading','verifying','ready','installing','handoff'].includes(value.status)) {
      value = {...value,status:'idle',error:null};
    }
    latest = value;
    window.TokenMarketDownloadProgress.render(progress, value.status === 'downloading', value.downloadProgress);
    recovery.disabled = !value.recoveryAvailable;
    el('client-recovery-help').hidden = Boolean(value.recoveryAvailable);
    currentVersion = value.currentVersion;
    el('client-current-version').textContent = `当前版本 ${value.currentVersion}`;
    el('client-update-status').textContent = value.status === 'available'
      ? `发现新版本 ${value.availableVersion}` : value.status === 'installing'
        ? '正在检查活动操作并准备受保护的安装交接…' : value.status === 'handoff'
          ? '安装交接已启动，等待安装及新版本启动确认' : value.status === 'current'
        ? '没有更新的受支持稳定版本' : value.status === 'downloading'
          ? '正在下载客户端更新…' : value.status === 'verifying'
            ? '下载完成，正在验证更新包签名…' : value.status === 'ready'
              ? `更新包 ${value.availableVersion} 签名验证通过，等待安装` : value.status === 'install_failed'
                ? `客户端安装失败：${value.error || '请重新检查更新或使用官方安装包恢复'}` : value.status === 'failed'
                ? `${value.availableVersion ? '下载或验证失败' : '检查失败'}：${value.error || '请重试'}` : '';
    el('client-update-status').className = ['failed','install_failed'].includes(value.status) ? 'error' : '';
    check.textContent = value.status === 'handoff' || (['failed','install_failed'].includes(value.status) && !value.availableVersion) ? '重试检查客户端更新' : '检查客户端更新';
    const busy = ['downloading','verifying','ready','installing'].includes(value.status);
    check.disabled = checking || busy;
    download.hidden = !value.availableVersion || ['ready','installing','handoff'].includes(value.status);
    download.disabled = busy || value.status === 'handoff';
    download.textContent = value.status === 'failed' ? '重试下载客户端更新' : '下载客户端更新';
    install.hidden = !['ready','installing'].includes(value.status);
    install.disabled = installing || value.status !== 'ready' || !value.installAvailable;
    el('client-install-unavailable').hidden = value.status !== 'ready' || Boolean(value.installAvailable);
    el('client-install-error').textContent = ['ready','handoff'].includes(value.status) ? value.error || '' : '';
    el('client-install-error').hidden = !el('client-install-error').textContent;
    el('client-update-notes').textContent = value.releaseNotes || '';
    el('client-update-notes').hidden = !value.releaseNotes;
  }
  check.onclick = async () => {
    if (checking) return;
    document.dispatchEvent(new Event('client-update-manual-check'));
    checking = true; check.disabled = true;
    el('client-update-status').textContent = '正在检查客户端更新…';
    try { publish(await invoke('check_client_update'), 'check'); }
    catch (error) { publish(latest?.status === 'handoff' ? {...latest,error:String(error)} : {status:'failed',currentVersion,error:String(error)}, 'check'); }
    finally { checking = false; render(latest); }
  };
  document.addEventListener('client-update-status', event => render(event.detail));
})();
