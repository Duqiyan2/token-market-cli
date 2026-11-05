/* Client maintenance stays available without a Client Session or workspace. */
(() => {
  if (!window.__TAURI__) return;
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const publish = value => document.dispatchEvent(new CustomEvent('client-update-action-result', {detail:value}));
  const el = id => document.getElementById(id);
  const check = el('check-client-update');
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
    render({...latest,status:'downloading',error:null});
    try { publish(await invoke('download_client_update', {version})); }
    catch (error) { publish({...latest,status:'failed',currentVersion,error:String(error)}); }
    finally { downloading = false; }
  };
  function render(value) {
    if (value.automatic && !['available','downloading','verifying','ready'].includes(value.status)) {
      value = {...value,status:'idle',error:null};
    }
    if (latest?.status === 'ready' && value.status !== 'ready') return;
    latest = value;
    currentVersion = value.currentVersion;
    el('client-current-version').textContent = `当前版本 ${value.currentVersion}`;
    el('client-update-status').textContent = value.status === 'available'
      ? `发现新版本 ${value.availableVersion}` : value.status === 'current'
        ? '没有更新的受支持稳定版本' : value.status === 'downloading'
          ? '正在下载客户端更新…' : value.status === 'verifying'
            ? '下载完成，正在验证更新包签名…' : value.status === 'ready'
              ? `更新包 ${value.availableVersion} 签名验证通过，等待安装` : value.status === 'failed'
                ? `${value.availableVersion ? '下载或验证失败' : '检查失败'}：${value.error || '请重试'}` : '';
    el('client-update-status').className = value.status === 'failed' ? 'error' : '';
    check.textContent = value.status === 'failed' && !value.availableVersion ? '重试检查客户端更新' : '检查客户端更新';
    const busy = ['downloading','verifying','ready'].includes(value.status);
    check.disabled = checking || busy;
    download.hidden = !value.availableVersion || value.status === 'ready';
    download.disabled = busy;
    download.textContent = value.status === 'failed' ? '重试下载客户端更新' : '下载客户端更新';
    el('install-client-update').hidden = value.status !== 'ready';
    el('client-install-unavailable').hidden = value.status !== 'ready';
    el('client-update-notes').textContent = value.releaseNotes || '';
    el('client-update-notes').hidden = !value.releaseNotes;
  }
  check.onclick = async () => {
    if (checking) return;
    document.dispatchEvent(new Event('client-update-manual-check'));
    checking = true; check.disabled = true;
    el('client-update-status').textContent = '正在检查客户端更新…';
    try { publish(await invoke('check_client_update')); }
    catch (error) { publish({status:'failed',currentVersion,error:String(error)}); }
    finally { checking = false; render(latest); }
  };
  document.addEventListener('client-update-status', event => render(event.detail));
})();
