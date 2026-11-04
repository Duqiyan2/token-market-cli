/* Window preferences belong to this device, including while signed out. */
(() => {
  if (!window.__TAURI__) return;
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const el = id => document.getElementById(id);
  const preferences = el('settings-page');
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

  let readGeneration = 0;
  let reading = false, loaded = false, available = false, savedBackground = false;
  const themeButtons = [...document.querySelectorAll('[data-theme-preference]')];
  const cancel = el('desktop-preferences-cancel');
  const retry = el('desktop-preferences-retry');
  function settingsFeedback(message, error = false) {
    el('desktop-preferences-feedback').textContent = message;
    el('desktop-preferences-feedback').className = `settings-feedback${error ? ' error' : ''}`;
  }
  function updateControls() {
    background.disabled = saving || reading || !loaded || !available;
    save.disabled = saving || reading || !loaded;
    cancel.disabled = saving || reading;
    themeButtons.forEach(button => { button.disabled = saving; });
    el('desktop-quit').disabled = saving;
  }
  async function readPreferences() {
    const generation = ++readGeneration;
    reading = true; loaded = false; retry.hidden = true;
    settingsFeedback('正在读取设置…'); updateControls();
    try {
      const value = await invoke('desktop_window_preferences');
      if (generation !== readGeneration || preferences.hidden) return;
      savedBackground = value.run_in_background; available = value.background_available; loaded = true;
      background.checked = savedBackground;
      settingsFeedback(value.error || (available ? '' : '当前无法使用后台驻留，关闭窗口将退出应用。请重新打开应用后重试。'), Boolean(value.error));
    } catch (error) {
      if (generation !== readGeneration || preferences.hidden) return;
      settingsFeedback(String(error), true); retry.hidden = false;
    } finally {
      if (generation === readGeneration) { reading = false; updateControls(); }
    }
  }
  function cancelSettings() {
    window.TokenMarketTheme.cancel();
    if (loaded) background.checked = savedBackground;
    settingsFeedback('');
  }
  function showPreferences() { window.TokenMarketShell.show('settings'); }
  window.addEventListener('tokenmarket-page-changed', event => { if (event.detail.to === 'settings') void readPreferences(); });
  window.addEventListener('tokenmarket-page-changing', event => {
    if (event.detail.from !== 'settings' || event.detail.to === 'settings') return;
    if (saving) { event.preventDefault(); return; }
    ++readGeneration; reading = false; cancelSettings();
  });
  document.querySelectorAll('[data-desktop-settings]').forEach(button => { button.onclick = showPreferences; });
  cancel.onclick = cancelSettings;
  retry.onclick = () => void readPreferences();
  save.onclick = async () => {
    if (saving || reading || !loaded) return;
    saving = true; settingsFeedback('正在保存…'); updateControls();
    const nextBackground = background.checked;
    try {
      await invoke('save_desktop_window_preferences', {runInBackground:nextBackground});
      savedBackground = nextBackground;
      window.TokenMarketTheme.save();
      settingsFeedback('已保存');
    } catch (error) { settingsFeedback(String(error), true); }
    finally { saving = false; updateControls(); }
  };
  el('desktop-quit').onclick = async () => {
    try { await invoke('quit_desktop_app'); }
    catch (error) { settingsFeedback(String(error), true); }
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
