/* All dashboard synchronization starts from lifecycle/user events, never a poll. */
(() => {
  let timer = null, running = false, requested = false, usageRequested = false;
  async function run() {
    timer = null;
    if (document.hidden) return;
    if (running) { requested = true; return; }
    running = true;
    const refreshUsage = usageRequested;
    usageRequested = false;
    try {
      await window.TokenMarketSession.sync();
      await Promise.allSettled([
        window.TokenMarketWorkspace.refresh(),
        window.TokenMarketTools.sync(),
      ]);
      // Activation only syncs account/tools. Usage refreshes on page/workspace changes.
      // Usage owns its request queue. Slow log queries must not block later
      // page/workspace lifecycle events from syncing the rest of the dashboard.
      if (refreshUsage) void window.TokenMarketUsage?.refresh();
    } finally {
      running = false;
      if (requested) { requested = false; request(); }
    }
  }
  function request(refreshUsage = false) {
    usageRequested ||= refreshUsage;
    clearTimeout(timer);
    // Focus, native activation and visibility often describe the same action.
    timer = setTimeout(run, 100);
  }
  window.addEventListener('focus', () => request());
  window.addEventListener('pageshow', event => { if (event.persisted) request(); });
  document.addEventListener('visibilitychange', () => { if (!document.hidden) request(); });
  window.addEventListener('tokenmarket-page-changed', event => {
    if (event.detail.from !== event.detail.to) request(event.detail.to === 'usage');
  });
  window.addEventListener('current-workspace-changed', () => request(true));
  window.__TAURI__?.event.listen('desktop-activated', () => request());
})();
