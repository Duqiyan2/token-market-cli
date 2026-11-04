/* Window preferences belong to this device, including while signed out. */
(() => {
  if (!window.__TAURI__) return;
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const el = id => document.getElementById(id);
  const preferences = el('desktop-preferences-dialog');
  const notice = el('desktop-close-dialog');
  const background = el('desktop-run-in-background');
  const save = el('desktop-preferences-save');
  const confirm = el('desktop-close-confirm');
  const cancelClose = el('desktop-close-cancel');
  let saving = false;
  let acknowledging = false;

  function showNotice() {
    if (notice.open) return;
    el('desktop-close-feedback').textContent = '';
    notice.showModal();
  }

  async function showPreferences() {
    if (preferences.open) return;
    background.disabled = true;
    save.disabled = true;
    el('desktop-preferences-feedback').textContent = '正在读取设置…';
    preferences.showModal();
    try {
      const value = await invoke('desktop_window_preferences');
      background.checked = value.run_in_background;
      background.disabled = !value.background_available;
      save.disabled = false;
      el('desktop-preferences-feedback').textContent = value.error || (value.background_available ? '' : '当前无法使用后台驻留，关闭窗口将退出应用。请重新打开应用后重试。');
    } catch (error) {
      el('desktop-preferences-feedback').textContent = String(error);
    }
  }

  document.querySelectorAll('[data-desktop-settings]').forEach(button => {
    button.onclick = showPreferences;
  });
  el('desktop-preferences-cancel').onclick = () => { if (!saving) preferences.close(); };
  preferences.addEventListener('cancel', event => { if (saving) event.preventDefault(); });
  save.onclick = async () => {
    if (saving) return;
    saving = true;
    save.disabled = true;
    try {
      await invoke('save_desktop_window_preferences', {runInBackground:background.checked});
      preferences.close();
    } catch (error) {
      el('desktop-preferences-feedback').textContent = String(error);
    } finally {
      saving = false;
      save.disabled = false;
    }
  };
  el('desktop-quit').onclick = async () => {
    try { await invoke('quit_desktop_app'); }
    catch (error) { el('desktop-preferences-feedback').textContent = String(error); }
  };

  async function cancelNotice() {
    try {
      await invoke('cancel_desktop_close');
      notice.close();
    } catch (error) { el('desktop-close-feedback').textContent = String(error); }
  }
  cancelClose.onclick = () => { if (!acknowledging) cancelNotice(); };
  notice.addEventListener('cancel', event => {
    event.preventDefault();
    if (!acknowledging) cancelNotice();
  });
  confirm.onclick = async () => {
    if (acknowledging) return;
    acknowledging = true;
    confirm.disabled = true;
    cancelClose.disabled = true;
    try {
      await invoke('acknowledge_desktop_close');
      notice.close();
    } catch (error) {
      el('desktop-close-feedback').textContent = String(error);
    } finally {
      acknowledging = false;
      confirm.disabled = false;
      cancelClose.disabled = false;
    }
  };

  (async () => {
    try {
      await window.__TAURI__.event.listen('desktop-show-preferences', showPreferences);
      await window.__TAURI__.event.listen('desktop-close-notice', showNotice);
      // A native close can happen before the webview has registered its listeners.
      const value = await invoke('desktop_window_preferences');
      if (value.close_notice_pending) showNotice();
    } catch (error) {
      // Keep the settings entry available so the user can inspect/retry IPC errors.
      console.error('Unable to initialize window preferences', error);
    }
  })();
})();
