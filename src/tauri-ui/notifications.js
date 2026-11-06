/* Stack visible feedback without reparenting live regions or shifting content. */
(() => {
  const selector = '#feedback, #session-feedback, .tools-feedback, .tools-updates-status, .workspace-status[data-feedback], #desktop-preferences-notice, #usage-feedback, .client-update-failure-notice';
  const observed = new Set();
  let scheduled = false;
  function schedule() {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(layout);
  }
  const resize = new ResizeObserver(schedule);
  function layout() {
    scheduled = false;
    const nodes = [...document.querySelectorAll(selector)];
    for (const node of observed) {
      if (!node.isConnected) { resize.unobserve(node); observed.delete(node); }
    }
    let bottom = 16;
    for (const node of nodes) {
      if (!observed.has(node)) { observed.add(node); resize.observe(node); }
      if (!node.getClientRects().length) continue;
      node.style.bottom = `${bottom}px`;
      bottom += node.getBoundingClientRect().height + 8;
    }
  }
  // Text, error styling and view visibility can all change independently.
  new MutationObserver(schedule).observe(document.body, {
    subtree:true, childList:true, characterData:true,
    attributes:true, attributeFilter:['hidden', 'class'],
  });
  window.addEventListener('resize', schedule);
  schedule();
})();
