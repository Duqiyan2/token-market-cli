/* All dashboard synchronization starts from lifecycle/user events, never a poll. */
(() => {
  let timer = null, running = false, requested = false;
  async function run() {
    timer = null;
    if (document.hidden) return;
    if (running) { requested = true; return; }
    running = true;
    try {
      await window.TokenMarketSession.sync();
      await Promise.allSettled([
        window.TokenMarketWorkspace.refresh(),
        window.TokenMarketTools.sync(),
      ]);
    } finally {
      running = false;
      if (requested) { requested = false; request(); }
    }
  }
  function request() {
    clearTimeout(timer);
    // Focus, native activation and visibility often describe the same action.
    timer = setTimeout(run, 100);
  }
  window.addEventListener('focus', request);
  window.addEventListener('pageshow', event => { if (event.persisted) request(); });
  document.addEventListener('visibilitychange', () => { if (!document.hidden) request(); });
  window.addEventListener('tokenmarket-page-changed', event => {
    if (event.detail.from !== event.detail.to) request();
  });
  window.addEventListener('current-workspace-changed', request);
  window.__TAURI__?.event.listen('desktop-activated', request);
})();
