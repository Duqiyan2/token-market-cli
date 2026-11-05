/* Startup reminders belong to the native process; reloads only reconcile its status. */
(() => {
  if (!window.__TAURI__) return;
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const el = id => document.getElementById(id);
  const dialog = el('client-update-reminder');
  let version;
  let revision = 0;
  let busy = false;
  let verifiedReady = false;
  let handedOff = false;
  let failureAnnounced = false;
  const failureNotice = el('client-update-failure-notice');
  el('client-update-failure-dismiss').onclick = () => { failureNotice.hidden = true; };
  el('client-update-failure-view').onclick = () => {
    failureNotice.hidden = true;
    document.querySelector('[data-desktop-settings]').click();
  };
  document.addEventListener('client-update-manual-check', () => { ++revision; });
  document.addEventListener('client-update-action-result', event => { ++revision; receive(event.detail); });
  function receive(value) {
    // Native keeps the verified candidate until guarded installation is available.
    if (handedOff && value.status !== 'handoff') return;
    if (verifiedReady && !['ready','installing','handoff'].includes(value.status)) return;
    if (['ready','installing','handoff'].includes(value.status)) verifiedReady = true;
    if (value.status === 'handoff') handedOff = true;
    document.dispatchEvent(new CustomEvent('client-update-status', {detail:value}));
    if (value.status === 'install_failed' && value.showRecoveryNotice && !failureAnnounced) {
      failureAnnounced = true;
      el('client-update-failure-message').textContent = value.error || '上次客户端安装未完成，请查看更新与恢复说明。';
      failureNotice.hidden = false;
      // Native acknowledgement persists for this application process, so a
      // renderer reload does not repeat an already displayed notice.
      invoke('acknowledge_client_update_failure').catch(() => {});
    } else if (value.status !== 'install_failed') failureNotice.hidden = true;
    if (value.status === 'available' && value.automatic && value.remind) {
      version = value.availableVersion;
      el('client-update-reminder-version').textContent = `Token Market ${version}`;
      el('client-update-reminder-notes').textContent = value.releaseNotes || '';
      if (!dialog.open) dialog.showModal();
    } else if (dialog.open) dialog.close();
  }
  async function dismiss(ignore, view) {
    if (busy) return;
    busy = true;
    el('client-update-reminder-error').textContent = '';
    try {
      receive(await invoke(ignore ? 'ignore_client_update_version' : 'defer_client_update', ignore ? {version} : undefined));
      dialog.close();
      if (view) document.querySelector('[data-desktop-settings]').click();
    } catch (error) { el('client-update-reminder-error').textContent = String(error); }
    finally { busy = false; }
  }
  el('client-update-later').onclick = () => dismiss(false, false);
  el('client-update-ignore').onclick = () => dismiss(true, false);
  el('client-update-view').onclick = () => dismiss(false, true);
  dialog.addEventListener('cancel', event => { event.preventDefault(); dismiss(false, false); });
  (async () => {
    const initialRevision = revision;
    await window.__TAURI__.event.listen('desktop-client-update-status', event => { ++revision; receive(event.payload); });
    const value = await invoke('desktop_client_update_status');
    if (initialRevision === revision) receive(value);
  })().catch(() => { /* An automatic check/status failure must not interrupt startup. */ });
})();
