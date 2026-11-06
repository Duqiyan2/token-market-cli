// Browser layout checks use isolated IPC; no real tools or credentials are touched.
const {test, before, after} = require('node:test');
const assert = require('node:assert/strict');
const {chromium, webkit} = require('playwright');
const openFixture = require('./helpers/desktop-fixture.cjs');
let browser;
before(async () => { browser = await (process.env.TOKEN_MARKET_TEST_BROWSER_ENGINE === 'webkit' ? webkit : chromium).launch({headless:true}); });
after(async () => { await browser?.close(); });

test('opening a connected tool succeeds silently and keeps content in place', async () => {
  const f = await openFixture(browser);
  try {
    const p = f.page;
    const before = await p.locator('.tools-list').boundingBox();
    await p.locator('[data-tool="claude-code"] .tool-launch').click();
    await p.waitForFunction(() => window.__fixture.calls.some(c => c.command === 'launch_connected_tool'));
    assert.equal(await p.locator('#tools-region > .tools-feedback').textContent(), '');
    assert.deepEqual(await p.locator('.tools-list').boundingBox(), before);
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

for (const width of [390, 900, 1440]) {
  test(`feedback never moves page content at ${width}px`, async () => {
    const f = await openFixture(browser);
    try {
      const p = f.page;
      await p.setViewportSize({width, height:900});
      for (const [view, sources, targets] of [
        ['tools', ['.tools-feedback', '.tools-updates-status', '#workspace-region [data-feedback]', '#session-feedback', '.client-update-failure-notice'], ['.tools-controls', '.tools-list', '.page-note']],
        ['detail', ['.tool-operation-status'], ['.tool-detail-heading', '.tool-detail-page']],
        ['account', ['#workspace-account-region [data-feedback]'], ['.account-overview', '.account-section']],
        ['settings', ['#desktop-preferences-feedback'], ['.settings-savebar', '.settings-session']],
        ['usage', ['#usage-feedback'], ['.usage-overview', '.usage-log-panel']],
      ]) {
        if (view === 'detail') await p.locator('[data-tool="claude-code"] .tool-detail').click();
        else await p.evaluate(view => window.TokenMarketShell.show(view), view);
        await p.locator(targets[0]).waitFor();
        if (view === 'usage') {
          await p.locator('#usage-rows[aria-busy="false"] [data-request-id="usage-0"]').waitFor();
          await p.locator('.usage-overview[aria-busy="false"]').waitFor();
        }
        // Exercise the actual status elements, including wrapping and clearing.
        const result = await p.evaluate(async ({sources, targets, view}) => {
          const elements = sources.map(s => document.querySelector(s));
          const snapshot = () => targets.map(s => {
            const r = document.querySelector(s).getBoundingClientRect();
            return [r.x, r.y, r.width, r.height];
          });
          const frame = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          elements.forEach(n => { n.textContent = ''; n.hidden = true; });
          const retry = view === 'settings' ? document.getElementById('desktop-preferences-retry') : null;
          if (retry) retry.hidden = true;
          await frame(); const before = snapshot();
          elements.forEach(n => { n.textContent = '操作提示：' + '这是一条需要换行的状态消息。'.repeat(16); n.hidden = false; });
          if (retry) retry.hidden = false;
          await frame(); const shown = snapshot();
          const boxes = elements.map(n => { const r = (n.closest('#desktop-preferences-notice') || n).getBoundingClientRect(); return {x:r.x,y:r.y,right:r.right,bottom:r.bottom}; });
          elements.forEach(n => { n.textContent = ''; n.hidden = true; });
          if (retry) retry.hidden = true;
          await frame();
          return {before, shown, cleared:snapshot(), boxes};
        }, {sources, targets, view});
        assert.deepEqual(result.shown, result.before, `${view}: showing feedback moved content`);
        assert.deepEqual(result.cleared, result.before, `${view}: clearing feedback moved content`);
        for (const box of result.boxes) {
          assert.ok(box.x >= 0 && box.right <= width && box.y >= 0 && box.bottom <= 900, `${view}: feedback escaped viewport`);
        }
        for (let i=0; i<result.boxes.length; i++) for (let j=i+1; j<result.boxes.length; j++) {
          assert.ok(result.boxes[i].bottom <= result.boxes[j].y || result.boxes[j].bottom <= result.boxes[i].y, `${view}: feedback overlaps`);
        }
      }
      assert.deepEqual(f.errors, []);
    } finally { await f.close(); }
  });
}

test('login feedback does not recenter the login form', async () => {
  const f = await openFixture(browser, {signedIn:false});
  try {
    const p = f.page;
    await p.locator('#begin').waitFor();
    const before = await p.locator('#begin').boundingBox();
    await p.evaluate(() => feedback('登录提示。'.repeat(40), true));
    assert.deepEqual(await p.locator('#begin').boundingBox(), before);
    await p.evaluate(() => feedback(''));
    assert.deepEqual(await p.locator('#begin').boundingBox(), before);
  } finally { await f.close(); }
});
