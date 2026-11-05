/* Reconcile trusted, locally constructed DOM without replacing stable controls. */
(() => {
  const key = node => node.nodeType === Node.ELEMENT_NODE
    ? node.getAttribute('data-live-key') || node.id || node.getAttribute('data-tool') || node.getAttribute('data-workspace-id')
    : null;
  const compatible = (current, next) => current.nodeType === next.nodeType
    && current.nodeName === next.nodeName && key(current) === key(next);
  function patch(current, next) {
    if (current.nodeType !== Node.ELEMENT_NODE) {
      if (current.nodeValue !== next.nodeValue) current.nodeValue = next.nodeValue;
      return;
    }
    // An open disclosure belongs to the user, not to the latest data snapshot.
    const preserve = name => current.tagName === 'DETAILS' && name === 'open';
    for (const attribute of [...current.attributes]) {
      if (!preserve(attribute.name) && !next.hasAttribute(attribute.name)) current.removeAttribute(attribute.name);
    }
    for (const attribute of next.attributes) {
      if (!preserve(attribute.name) && current.getAttribute(attribute.name) !== attribute.value) current.setAttribute(attribute.name, attribute.value);
    }
    // Event handlers must use event.currentTarget for local DOM references;
    // their data closures are replaced so retained controls use fresh values.
    for (const name of ['onclick', 'oninput', 'onchange', 'onkeydown']) current[name] = next[name];
    children(current, next);
    if (current instanceof HTMLInputElement) {
      if (current.value !== next.value) current.value = next.value;
      if (current.checked !== next.checked) current.checked = next.checked;
    } else if (current instanceof HTMLSelectElement && current.value !== next.value) current.value = next.value;
  }
  function children(parent, desired) {
    const focused = parent.contains(document.activeElement) ? document.activeElement : null;
    const previous = [...parent.childNodes], used = new Set();
    const keyed = new Map(previous.filter(node => key(node)).map(node => [key(node), node]));
    let cursor = parent.firstChild;
    for (const next of [...desired.childNodes]) {
      let current = key(next) ? keyed.get(key(next)) : previous.find(node => !used.has(node) && !key(node) && compatible(node, next));
      if (current && (used.has(current) || !compatible(current, next))) current = null;
      if (current) { used.add(current); patch(current, next); }
      else current = next;
      if (current !== cursor) parent.insertBefore(current, cursor);
      cursor = current.nextSibling;
    }
    for (const node of previous) if (!used.has(node)) node.remove();
    if (focused?.isConnected && document.activeElement === document.body && !focused.disabled) focused.focus({preventScroll: true});
  }
  window.TokenMarketLiveUI = { children };
})();
