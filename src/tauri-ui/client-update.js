/* Client maintenance stays available without a Client Session or workspace. */
(() => {
  if (!window.__TAURI__) return;
  const invoke = command => window.__TAURI__.core.invoke(command);
  const el = id => document.getElementById(id);
  const check = el('check-client-update');
  let checking = false;
  let currentVersion = '未知';
  function render(value) {
    if (value.automatic && value.status !== 'available') {
      value = {...value, status:'idle', error:null};
    }
    currentVersion = value.currentVersion;
    el('client-current-version').textContent = `当前版本 ${value.currentVersion}`;
    el('client-update-status').textContent = value.status === 'available'
      ? `发现新版本 ${value.availableVersion}` : value.status === 'current'
        ? '没有更新的受支持稳定版本' : value.status === 'failed'
          ? `检查失败：${value.error || '无法读取稳定版清单，请重试'}` : '';
    el('client-update-status').className = value.status === 'failed' ? 'error' : '';
    check.textContent = value.status === 'failed' ? '重试检查客户端更新' : '检查客户端更新';
    el('client-update-notes').textContent = value.releaseNotes || '';
    el('client-update-notes').hidden = !value.releaseNotes;
  }
  check.onclick = async () => {
    if (checking) return;
    document.dispatchEvent(new Event('client-update-manual-check'));
    checking = true; check.disabled = true;
    el('client-update-status').textContent = '正在检查客户端更新…';
    try { render(await invoke('check_client_update')); }
    catch (error) { render({status:'failed',currentVersion,error:String(error)}); }
    finally { checking = false; check.disabled = false; }
  };
  document.addEventListener('client-update-status', event => render(event.detail));
})();
