/* Run before CSS so saved appearance is applied before the first paint. */
(() => {
  const key = 'tokenmarket.theme';
  const valid = value => ['light', 'dark', 'system'].includes(value);
  const system = window.matchMedia('(prefers-color-scheme: dark)');
  let saved = 'system';
  try { const value = localStorage.getItem(key); if (valid(value)) saved = value; } catch (_) {}
  let preference = saved;
  function apply() {
    const effective = preference === 'system' ? (system.matches ? 'dark' : 'light') : preference;
    document.documentElement.dataset.theme = effective;
    document.documentElement.style.colorScheme = effective;
    const nativeWindow = window.__TAURI__?.window?.getCurrentWindow?.();
    nativeWindow?.setTheme(preference === 'system' ? null : preference).catch(() => {});
  }
  function sync() {
    document.querySelectorAll('[data-theme-preference]').forEach(button => {
      const selected = button.dataset.themePreference === preference;
      button.setAttribute('aria-checked', String(selected));
      button.tabIndex = selected ? 0 : -1;
    });
    apply();
  }
  window.TokenMarketTheme = {
    get preference() { return preference; },
    save() {
      saved = preference;
      try { localStorage.setItem(key, saved); } catch (_) {}
    },
    cancel() { preference = saved; sync(); },
  };
  system.addEventListener('change', () => { if (preference === 'system') apply(); });
  window.addEventListener('storage', event => {
    if (event.key !== key && event.key !== null) return;
    saved = valid(event.newValue) ? event.newValue : 'system';
    preference = saved; sync();
  });
  apply();
  document.addEventListener('DOMContentLoaded', () => {
    sync();
    const buttons = [...document.querySelectorAll('[data-theme-preference]')];
    buttons.forEach((button, index) => {
      button.addEventListener('click', () => { preference = button.dataset.themePreference; sync(); });
      button.addEventListener('keydown', event => {
        if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End'].includes(event.key)) return;
        event.preventDefault();
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (['ArrowRight','ArrowDown'].includes(event.key) ? 1 : -1) + buttons.length) % buttons.length;
        if (!buttons[next].disabled) { buttons[next].focus(); buttons[next].click(); }
      });
    });
  });
})();
