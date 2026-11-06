/* Startup reminders belong to the native process; reloads only reconcile its status. */
(() => {
  if (!window.__TAURI__) return;
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const el = id => document.getElementById(id);
  const dialog = el('client-update-reminder');
  let version;
  let revision = 0;
  let busy = false;
  let latest = null;
  let nativeProgress = true;
  let allowHandoff = true;
  let failureAnnounced = false;
  const failureNotice = el('client-update-failure-notice');
  el('client-update-failure-dismiss').onclick = () => { failureNotice.hidden = true; };
  el('client-update-failure-view').onclick = () => {
    failureNotice.hidden = true;
    document.querySelector('[data-desktop-settings]').click();
  };
  document.addEventListener('client-update-manual-check', () => { ++revision; });
  document.addEventListener('client-update-action-result', event => { ++revision; receive(event.detail.value, event.detail.action); });
  function receive(value, action) {
    // One gate owns both Settings and reminders. Explicit native results can
    // resolve handoff; unsolicited old progress cannot do so.
    const recovery = action === 'check' && latest?.status === 'handoff';
    // A returned native ready result proves this installation is inactive.
    // It wins over an old handoff event; an IPC error makes no such claim.
    const rejectedInstall = action === 'install-result' && value.status === 'ready';
    if (!action && (!nativeProgress || (!allowHandoff && value.status === 'handoff'))) return;
    if (latest?.status === 'handoff' && value.status !== 'handoff' && !recovery && !rejectedInstall) return;
    if (['ready','installing'].includes(latest?.status) && !['ready','installing','handoff'].includes(value.status)) return;
    if (recovery && value.status !== 'handoff') {
      // All queued native events belong to the resolved attempt. A later
      // download permits new progress; only an explicit install permits handoff.
      nativeProgress = false;
      allowHandoff = false;
    }
    if (action === 'download') nativeProgress = true;
    if (action === 'install' && value.status === 'installing') {
      nativeProgress = true;
      allowHandoff = true;
    }
    if (rejectedInstall) allowHandoff = false;
    latest = value;
    document.dispatchEvent(new CustomEvent('client-update-status', {detail:value}));
    if (value.status === 'install_failed' && value.showRecoveryNotice && !failureAnnounced) {
      failureAnnounced = true;
      el('client-update-failure-message').textContent = value.error || '上次客户端安装未完成，请重新检查更新。';
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
