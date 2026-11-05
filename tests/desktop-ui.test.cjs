// Run with Node + Playwright installed. All native IPC is isolated in the page;
// these tests never load credentials, change tool files, launch tools or bill a model.
const {test, before, after} = require('node:test');
const assert = require('node:assert/strict');
const {chromium} = require('playwright');
let browser;
before(async () => { browser = await chromium.launch({headless:true, ...(process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL ? {channel:process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL} : {})}); });
after(async () => { await browser?.close(); });
const createFixture = require('./helpers/desktop-fixture.cjs');
const openFixture = options => createFixture(browser, options);
const calls = (page,command) => page.evaluate(command => window.__fixture.calls.filter(call=>call.command===command),command);
async function detail(page,id) { await page.locator(`[data-tool="${id}"] .tool-detail`).click(); }
async function switchTeam(page) { await page.locator('.workspace-switch').click();await page.locator('dialog[open] [data-workspace-id="team"]').click();await page.locator('.workspace-switch').filter({hasText:'团队空间'}).waitFor(); }
async function manage(page) { await page.locator('.tool-menu summary').click(); }

test('signed-out Settings shows a Client Update and release notes without downloading', async () => {
  const f=await openFixture({signedIn:false,clientUpdate:{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',releaseNotes:'修复窗口恢复\n保留已有工具连接',error:null}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'发现新版本 1.1.0'}).waitFor();
    assert.match(await p.locator('#client-current-version').textContent(),/1\.0\.0/);
    assert.equal(await p.locator('#client-update-notes').textContent(),'修复窗口恢复\n保留已有工具连接');
    assert.equal((await calls(p,'check_client_update')).length,1);
    assert.equal((await p.evaluate(()=>window.__fixture.calls.filter(c=>/download|install|restart/.test(c.command)))).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('Client Update reports no newer supported stable version without workspace balance and stays separate from Tool Upgrades', async () => {
  const f=await openFixture({noBalance:true});try {
    const p=f.page;await p.locator('#signed-in [data-desktop-settings]').click();
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'没有更新的受支持稳定版本'}).waitFor();
    assert.equal(await p.locator('#client-update-notes').isVisible(),false);
    const toolChecks=(await calls(p,'check_tool_updates')).length;
    await p.locator('.primary-nav [data-page="tools"]').click();
    await p.getByRole('button',{name:'检查更新',exact:true}).click();
    await p.waitForFunction(n=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length>n,toolChecks);
    assert.equal((await calls(p,'check_client_update')).length,1);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('a failed manual Client Update check gives the reason and can retry', async () => {
  const f=await openFixture({signedIn:false,clientUpdate:{status:'failed',currentVersion:'1.0.0',availableVersion:null,releaseNotes:null,error:'稳定版清单暂时不可用'}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'稳定版清单暂时不可用'}).waitFor();
    await p.evaluate(()=>{window.__fixture.clientUpdate={status:'current',currentVersion:'1.0.0',availableVersion:null,releaseNotes:null,error:null};});
    await p.getByRole('button',{name:'重试检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'没有更新的受支持稳定版本'}).waitFor();
    assert.equal((await calls(p,'check_client_update')).length,2);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('a late initial Client Update status cannot erase a completed manual check', async () => {
  const f=await openFixture({signedIn:false,deferClientUpdateStatus:true,clientUpdate:{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',releaseNotes:'稳定版本',error:null}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'发现新版本 1.1.0'}).waitFor();
    await p.evaluate(()=>window.__fixture.resolveClientUpdateStatus());
    await p.waitForTimeout(30);
    assert.match(await p.locator('#client-update-status').textContent(),/发现新版本 1\.1\.0/);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('settings uses a full page from sidebar, login and native menu, including after session polling', async () => {
  for(const signedIn of [true,false]) {
    const f=await openFixture({signedIn});try {
      const p=f.page;await p.locator(`${signedIn?'#signed-in':'#login'} [data-desktop-settings]`).click();
      await p.locator('#desktop-run-in-background:enabled').waitFor();
      assert.equal(await p.locator('#settings-page').evaluate(node=>node.tagName),'SECTION');
      assert.equal(await p.locator('dialog[open]').count(),0);
      await p.evaluate(()=>window.__fixture.emit('desktop-login-status',window.__fixture.loginStatus()));
      assert.equal(await p.locator('#settings-page').isVisible(),true);assert.equal(await p.locator('#login').isVisible(),false);
      if(signedIn) { assert.equal(await p.locator('#tools-page').isVisible(),false);assert.equal(await p.locator('#signed-in [data-desktop-settings]').getAttribute('aria-current'),'page');await p.locator('.primary-nav [data-page="tools"]').click(); }
      else await p.locator('#settings-return-login').click();
      assert.equal(await p.locator('#settings-page').isVisible(),false);
      await p.evaluate(()=>window.__fixture.emit('desktop-show-preferences'));
      await p.locator('#desktop-run-in-background:enabled').waitFor();assert.equal(await p.locator('#settings-page').isVisible(),true);
      assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});

test('theme segments support arrow keys and leaving settings discards unsaved appearance', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('#signed-in [data-desktop-settings]').click();await p.locator('#desktop-run-in-background:enabled').waitFor();
    await p.locator('[data-theme-preference="system"]').focus();await p.keyboard.press('ArrowLeft');
    assert.equal(await p.locator('[data-theme-preference="dark"]').getAttribute('aria-checked'),'true');
    assert.equal(await p.locator('html').getAttribute('data-theme'),'dark');
    await p.locator('#desktop-run-in-background').uncheck();await p.locator('.primary-nav [data-page="account"]').click();
    assert.equal(await p.locator('html').getAttribute('data-theme'),'light');assert.equal((await calls(p,'save_desktop_window_preferences')).length,0);
    await p.locator('#signed-in [data-desktop-settings]').click();await p.locator('#desktop-run-in-background:enabled').waitFor();
    assert.equal(await p.locator('#desktop-run-in-background').isChecked(),true);
    assert.equal(await p.locator('[data-theme-preference="system"]').getAttribute('aria-checked'),'true');assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('a late settings read cannot reopen the page after navigating away', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.evaluate(()=>window.__fixture.deferPreferences=true);
    await p.locator('#signed-in [data-desktop-settings]').click();await p.waitForFunction(()=>Boolean(window.__fixture.resolvePreferences));
    await p.locator('.primary-nav [data-page="tools"]').click();await p.evaluate(()=>{window.__fixture.deferPreferences=false;window.__fixture.resolvePreferences();});
    assert.equal(await p.locator('#settings-page').isVisible(),false);assert.equal(await p.locator('#tools-page').isVisible(),true);
    await p.locator('#signed-in [data-desktop-settings]').click();await p.locator('#desktop-run-in-background:enabled').waitFor();assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('account menu is anchored, compact, keyboard accessible and dismisses without a modal backdrop', async () => {
  const f=await openFixture();try {
    const p=f.page;
    for (const width of [900,375]) {
      await p.setViewportSize({width,height:760});
      const trigger=p.locator('.workspace-profile-button');await trigger.click();
      const menu=p.locator('#account-menu');assert.equal(await menu.isVisible(),true);
      assert.equal(await p.evaluate(()=>Boolean(document.querySelector('#account-menu:modal'))),false);
      const box=await menu.boundingBox();assert.ok(box.width<=300 && box.height<370);
      assert.ok(box.x>=0 && box.y>=0 && box.x+box.width<=width && box.y+box.height<=760);
      assert.equal(await trigger.getAttribute('aria-expanded'),'true');
      await p.keyboard.press('ArrowDown');
      assert.equal(await p.evaluate(()=>document.activeElement.textContent),'账号与空间');
      await p.keyboard.press('Escape');assert.equal(await menu.isVisible(),false);
      assert.equal(await trigger.getAttribute('aria-expanded'),'false');
      await trigger.click();await p.locator('#account-menu').getByRole('button',{name:'账号与空间',exact:true}).click();
      await p.getByRole('heading',{name:'可切换空间'}).waitFor();assert.equal(await menu.isVisible(),false);
      await trigger.click();await p.locator('#signed-in .brand').click();assert.equal(await menu.isVisible(),false);
    }
    assert.equal((await calls(p,'logout_desktop_session')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('tool details replace the list and return to the active search without overlay fragments', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('.tools-search').fill('Claude');await detail(p,'claude-code');
    assert.equal(await p.locator('#tools-page').isVisible(),false);
    assert.equal(await p.locator('#tool-detail-page').isVisible(),true);
    assert.ok((await p.locator('.tool-detail-close').boundingBox()).y >= 62);
    assert.equal(await p.locator('#tool-detail-page').evaluate(node=>getComputedStyle(node).position),'static');
    await p.locator('.tool-detail-close').click();assert.equal(await p.locator('#tools-page').isVisible(),true);
    assert.equal(await p.locator('.tools-search').inputValue(),'Claude');assert.equal(await p.locator('.tool-row').count(),1);
    assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('appearance follows OS changes, explicit selection overrides OS, and saved preference survives reload', async () => {
  const f = await openFixture({colorScheme:'dark'}); try {
    const p=f.page;
    const theme=()=>p.locator('html').getAttribute('data-theme');
    assert.equal(await theme(),'dark');
    await p.emulateMedia({colorScheme:'light'}); await p.waitForFunction(()=>document.documentElement.dataset.theme==='light'); assert.equal(await theme(),'light');
    await p.locator('#signed-in [data-desktop-settings]').click();
    await p.locator('#desktop-run-in-background:enabled').waitFor();
    assert.equal(await p.locator('[data-theme-preference][aria-checked="true"]').getAttribute('data-theme-preference'),'system');
    await p.locator('[data-theme-preference=\"dark\"]').click();
    assert.equal(await theme(),'dark');
    await p.emulateMedia({colorScheme:'light'}); assert.equal(await theme(),'dark');
    await p.locator('#desktop-preferences-save').click();
    await p.locator('#desktop-preferences-feedback').filter({hasText:'已保存'}).waitFor();
    await p.reload(); await p.locator('.tool-row').first().waitFor();
    assert.equal(await theme(),'dark');
    assert.equal(await p.evaluate(()=>window.__fixture.nativeTheme),'dark');
    await p.locator('#signed-in [data-desktop-settings]').click();
    await p.locator('#desktop-run-in-background:enabled').waitFor();
    await p.locator('[data-theme-preference=\"system\"]').click();
    await p.locator('#desktop-preferences-save').click();
    await p.locator('#desktop-preferences-feedback').filter({hasText:'已保存'}).waitFor();
    await p.emulateMedia({colorScheme:'dark'}); await p.waitForFunction(()=>document.documentElement.dataset.theme==='dark'); assert.equal(await theme(),'dark');
    await p.emulateMedia({colorScheme:'light'}); await p.waitForFunction(()=>document.documentElement.dataset.theme==='light'); assert.equal(await theme(),'light');
    assert.equal(await p.evaluate(()=>window.__fixture.nativeTheme),null);
    assert.equal(await p.evaluate(()=>localStorage.getItem('tokenmarket.theme')),'system');
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('signed-out theme preview cancels without replacing the saved preference', async () => {
  const f=await openFixture({signedIn:false,storedTheme:'dark'});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#desktop-run-in-background:enabled').waitFor();
    await p.locator('[data-theme-preference=\"light\"]').click();
    assert.equal(await p.locator('html').getAttribute('data-theme'),'light');
    await p.locator('#settings-return-login').click();await p.locator('#settings-page').waitFor({state:'hidden'});
    assert.equal(await p.locator('html').getAttribute('data-theme'),'dark');
    assert.equal(await p.evaluate(()=>localStorage.getItem('tokenmarket.theme')),'dark');
    assert.equal(await p.evaluate(()=>window.__fixture.nativeTheme),'dark');
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('invalid theme falls back to system; failed settings save leaves theme uncommitted', async () => {
  const f=await openFixture({storedTheme:'invalid',preferenceSaveError:true});try {
    const p=f.page;assert.equal(await p.locator('html').getAttribute('data-theme'),'light');
    await p.locator('#signed-in [data-desktop-settings]').click();await p.locator('#desktop-run-in-background:enabled').waitFor();
    await p.locator('[data-theme-preference=\"dark\"]').click();await p.locator('#desktop-preferences-save').click();
    await p.locator('#desktop-preferences-feedback').filter({hasText:'无法保存'}).waitFor();
    assert.equal(await p.evaluate(()=>localStorage.getItem('tokenmarket.theme')),'invalid');
    await p.locator('#desktop-preferences-cancel').click();
    assert.equal(await p.locator('html').getAttribute('data-theme'),'light');assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('window preferences are available before login and saving does not log out or disconnect', async () => {
  const f = await openFixture({signedIn:false}); try {
    const p = f.page;
    await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#desktop-run-in-background:enabled').waitFor();
    assert.equal(await p.locator('#desktop-run-in-background').isChecked(), true);
    await p.locator('#desktop-run-in-background').uncheck();
    await p.locator('#desktop-preferences-save').click();
    await p.locator('#desktop-preferences-feedback').filter({hasText:'已保存'}).waitFor();
    assert.deepEqual((await calls(p, 'save_desktop_window_preferences'))[0].args, {runInBackground:false});
    await p.locator('#settings-return-login').click();
    await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#desktop-run-in-background:enabled').waitFor();
    assert.equal(await p.locator('#desktop-run-in-background').isChecked(), false);
    await p.locator('#desktop-quit').click();
    assert.equal((await calls(p, 'quit_desktop_app')).length, 1);
    assert.equal((await calls(p, 'logout_desktop_session')).length, 0);
    assert.equal((await calls(p, 'disconnect_tool')).length, 0);
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

test('first close is explained before hiding, including a close before listeners register', async () => {
  const f = await openFixture({closeNoticePending:true}); try {
    const p = f.page;
    await p.locator('#desktop-close-dialog[open]').waitFor();
    assert.equal((await calls(p, 'acknowledge_desktop_close')).length, 0);
    await p.evaluate(() => window.__fixture.emit('desktop-close-notice'));
    assert.equal(await p.locator('#desktop-close-dialog[open]').count(), 1);
    await p.locator('#desktop-close-confirm').click();
    await p.locator('#desktop-close-dialog').waitFor({state:'hidden'});
    assert.equal((await calls(p, 'acknowledge_desktop_close')).length, 1);
    assert.equal(await p.evaluate(() => window.__fixture.hidden), true);
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

test('cancelling the first-close notice keeps the window open and allows another close', async () => {
  const f = await openFixture(); try {
    const p = f.page;
    await p.evaluate(() => window.__fixture.emit('desktop-close-notice'));
    await p.locator('#desktop-close-dialog[open]').waitFor();
    await p.keyboard.press('Escape');
    await p.locator('#desktop-close-dialog').waitFor({state:'hidden'});
    assert.equal((await calls(p, 'cancel_desktop_close')).length, 1);
    assert.equal((await calls(p, 'acknowledge_desktop_close')).length, 0);
    await p.evaluate(() => window.__fixture.emit('desktop-close-notice'));
    await p.locator('#desktop-close-dialog[open]').waitFor();
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

test('failed preference persistence keeps the first-close notice visible and does not hide', async () => {
  const f = await openFixture({closeNoticePending:true,preferenceSaveError:true}); try {
    const p = f.page;
    await p.locator('#desktop-close-dialog[open]').waitFor();
    await p.locator('#desktop-close-confirm').click();
    await p.locator('#desktop-close-feedback').filter({hasText:'无法保存'}).waitFor();
    assert.equal(await p.locator('#desktop-close-dialog').isVisible(), true);
    assert.equal(await p.evaluate(() => Boolean(window.__fixture.hidden)), false);
    await p.locator('#desktop-close-cancel').click();
    await p.locator('#desktop-close-dialog').waitFor({state:'hidden'});
    await p.evaluate(() => window.__fixture.emit('desktop-show-preferences'));
    await p.locator('#desktop-run-in-background:enabled').waitFor();
    await p.locator('#desktop-run-in-background').uncheck();
    await p.locator('#desktop-preferences-save').click();
    await p.locator('#desktop-preferences-feedback').filter({hasText:'无法保存'}).waitFor();
    assert.equal(await p.locator('#settings-page').isVisible(), true);
    assert.equal(await p.evaluate(() => window.__fixture.windowPreferences.run_in_background), true);
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

test('unavailable tray disables background preference and keeps explicit quit available', async () => {
  const f = await openFixture({backgroundAvailable:false}); try {
    const p = f.page;
    await p.locator('#signed-in [data-desktop-settings]').click();
    await p.locator('#desktop-preferences-feedback').filter({hasText:'关闭窗口将退出应用'}).waitFor();
    assert.equal(await p.locator('#desktop-run-in-background').isDisabled(), true);
    await p.locator('#desktop-quit').click();
    assert.equal((await calls(p, 'quit_desktop_app')).length, 1);
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

test('other-environment connections require reconnect and cannot start a paid test', async () => {
  const f = await openFixture(); try {
    const p = f.page;
    await p.evaluate(() => {
      const entry = window.__fixture.entries.find(e => e.id === 'dumate');
      entry.installed = true; entry.canConnect = true;
      entry.connection = {...window.__fixture.entries[0].connection, state:'environment_mismatch',models:['glm-5.2'],testResult:null};
    });
    await p.evaluate(() => window.TokenMarketTools.refresh(true));
    await p.locator('[data-tool-filter="available"]').click();
    assert.equal(await p.locator('[data-tool="dumate"]').count(), 1);
    await p.locator('[data-tool-filter="connected"]').click();
    assert.equal(await p.locator('[data-tool="dumate"]').count(), 0);
    await p.locator('[data-tool-filter="all"]').click();
    await detail(p, 'dumate');
    assert.match(await p.locator('#tool-detail-region .notice').innerText(), /已接入其他环境/);
    assert.equal(await p.locator('#tool-detail-region .tool-test').isDisabled(), true);
    await p.evaluate(() => window.dispatchEvent(new CustomEvent('tokenmarket-test-request', {detail:{id:'dumate',connection:window.__fixture.entries.find(e=>e.id==='dumate').connection}})));
    assert.equal(await p.locator('.connection-test-dialog[open]').count(), 0);
    await manage(p);
    await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.reconnect-dialog[open]').waitFor();
    assert.equal((await calls(p, 'test_tool_connection')).length, 0);
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

test('DuMate personal installs separately, configures one model, and tests only on request', async () => {
  const f=await openFixture(); try {
    const p=f.page; const row=p.locator('[data-tool="dumate"]');
    await row.locator('img').evaluate(image=>image.decode());
    await row.locator('.tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    await p.locator('.tool-install-close').click();
    assert.equal((await calls(p,'connect_tool')).length,0);
    await detail(p,'dumate');
    assert.match(await p.locator('#tool-detail-region').innerText(),/一次配置一个模型/);
    await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.connect-confirm-dialog[open] .connect-confirm').click();
    await p.getByRole('heading',{name:'百度搭子 DuMate（个人版） 已接入'}).waitFor();
    assert.match(await p.locator('.restart-notice').innerText(),/完全退出并重新打开 DuMate/);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await p.locator('dialog[open] .close').click();
    await p.locator('#tool-detail-region .tool-test').click();
    const selector=p.locator('.connection-test-dialog select');
    assert.equal(await selector.isDisabled(),true);
    assert.equal(await selector.locator('option').count(),1);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'dumate',recovery:null});
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('Kimi Code installs and connects separately with its own tool identity', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    const row=p.locator('[data-tool="kimi-code"]');
    await row.locator('img').evaluate(image => image.decode());
    assert.equal(await row.locator('img').evaluate(image => image.naturalWidth > 0),true);
    await row.locator('.tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    await p.locator('.tool-install-close').click();
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await row.locator('.tool-connect').click();
    await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'Kimi Code 已接入'}).waitFor();
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'kimi-code',recovery:null});
    await p.locator('dialog[open] .close').click();
    await detail(p,'kimi-code');
    await p.locator('#tool-detail-region .tool-launch').click();
    assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'kimi-code');
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('Hermes installation and connection use its own identity without hidden paid tests', async () => {
  const f=await openFixture();try {
    const p=f.page;const row=p.locator('[data-tool="hermes"]');assert.equal(await row.count(),1);
    await row.locator('img').evaluate(image=>image.decode());
    await row.locator('.tool-install').click();await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();await p.locator('.tool-install-close').click();
    assert.equal((await calls(p,'connect_tool')).length,0);assert.equal((await calls(p,'test_tool_connection')).length,0);
    await row.locator('.tool-connect').click();await p.locator('.connect-confirm-dialog[open]').waitFor();await p.locator('.connect-confirm').click();await p.getByRole('heading',{name:'Hermes 已接入'}).waitFor();assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'hermes',recovery:null});await p.locator('dialog[open] .close').click();await detail(p,'hermes');assert.match(await p.locator('#tool-detail-region').innerText(),/Hermes/);assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('MiniMax Code installs and connects separately with its own tool identity', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    const row=p.locator('[data-tool="minimax-code"]');
    assert.equal(await row.count(),1);
    await row.locator('img').evaluate(image => image.decode());
    assert.equal(await row.locator('img').evaluate(image => image.naturalWidth > 0),true);
    await row.locator('.tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    await p.locator('.tool-install-close').click();
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await row.locator('.tool-connect').click();
    await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'MiniMax Code 已接入'}).waitFor();
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'minimax-code',recovery:null});
    await p.locator('dialog[open] .close').click();
    await detail(p,'minimax-code');
    await p.locator('#tool-detail-region .tool-launch').click();
    assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'minimax-code');
    await p.locator('.tool-test').click();
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.equal(await p.locator('.test-confirm').isDisabled(),true);
    await p.locator('.test-model').selectOption('server-model');
    await p.locator('.test-confirm').click();await p.locator('.test-feedback.success').waitFor();
    assert.deepEqual((await calls(p,'test_tool_connection'))[0].args,{tool:'minimax-code',expectedKeyId:'receipt-minimax-code',request:{confirmed:true,model:'server-model'}});
    await p.locator('.test-cancel').click();
    await manage(p);await p.locator('.tool-disconnect').click();
    await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入'}).click();
    await p.getByRole('heading',{name:'取消接入完成'}).waitFor();
    assert.equal((await calls(p,'disconnect_tool'))[0].args.tool,'minimax-code');
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('Oh My Pi installs and connects separately with its own tool identity', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    const row=p.locator('[data-tool="oh-my-pi"]');
    await row.locator('img').evaluate(image => image.decode());
    assert.equal(await row.locator('img').evaluate(image => image.naturalWidth > 0),true);
    await row.locator('.tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    await p.locator('.tool-install-close').click();
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await row.locator('.tool-connect').click();
    await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'Oh My Pi 已接入'}).waitFor();
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'oh-my-pi',recovery:null});
    await p.locator('dialog[open] .close').click();
    await detail(p,'oh-my-pi');
    await p.locator('#tool-detail-region .tool-launch').click();
    assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'oh-my-pi');
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('Pi installs and connects separately with its own tool identity', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    const row=p.locator('[data-tool="pi"]');
    await row.locator('img').evaluate(image => image.decode());
    assert.equal(await row.locator('img').evaluate(image => image.naturalWidth > 0),true);
    await row.locator('.tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    await p.locator('.tool-install-close').click();
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await row.locator('.tool-connect').click();
    await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'Pi 已接入'}).waitFor();
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'pi',recovery:null});
    await p.locator('dialog[open] .close').click();
    await detail(p,'pi');
    await p.locator('#tool-detail-region .tool-launch').click();
    assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'pi');
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('Grok Build installs and connects separately with its own tool identity', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    const row=p.locator('[data-tool="grok-build"]');
    await row.locator('img').evaluate(image => image.decode());
    assert.equal(await row.locator('img').evaluate(image => image.naturalWidth > 0),true);
    await row.locator('.tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    await p.locator('.tool-install-close').click();
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await row.locator('.tool-connect').click();
    await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'Grok Build 已接入'}).waitFor();
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'grok-build',recovery:null});
    await p.locator('dialog[open] .close').click();
    await detail(p,'grok-build');
    await p.locator('#tool-detail-region .tool-launch').click();
    assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'grok-build');
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('OpenClaw installs with progress and refreshes without implicitly connecting or testing', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    await p.locator('[data-tool="openclaw"] .tool-install').click();
    await p.locator('.tool-install-dialog[open]').waitFor();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    assert.match(await p.locator('.tool-install-output').innerText(),/运行依赖/);
    assert.equal((await calls(p,'start_tool_installation')).length,1);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await p.locator('.tool-install-close').click();
    await p.locator('[data-tool="openclaw"] .tool-connect').waitFor();
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('Claude Code and Codex install through shared actions without automatic connection or testing', async () => {
  const f=await openFixture({missingNativeCli:true}); try {
    const p=f.page;
    for(const tool of ['claude-code','codex']) {
      const install = p.locator(`[data-tool="${tool}"] .tool-install`);
      assert.equal(await install.isDisabled(),false);
      await install.click();
      await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
      await p.locator('.tool-install-close').click();
      await p.locator(`[data-tool="${tool}"] .tool-connect`).waitFor();
    }
    assert.deepEqual((await calls(p,'start_tool_installation')).map(call=>call.args.tool),['claude-code','codex']);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('installation can be cancelled and unsupported installs explain their disabled action', async () => {
  const f=await openFixture({installPending:true,unsupportedTool:'opencode'}); try {
    const p=f.page;
    const unsupported=p.locator('[data-tool="opencode"] .tool-install');
    assert.equal(await unsupported.isDisabled(),true);
    assert.match(await unsupported.getAttribute('title'),/兼容安装方案/);
    await p.locator('[data-tool="openclaw"] .tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'运行依赖'}).waitFor();
    await p.locator('.tool-install-cancel').click();
    await p.locator('.tool-install-status').filter({hasText:'安装已取消'}).waitFor();
    assert.equal(await p.locator('.tool-install-retry').isVisible(),true);
    assert.equal((await calls(p,'cancel_tool_installation')).length,1);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('GUI installation waits for native completion and can stop waiting without cancelling the native installer', async () => {
  const f=await openFixture({missingGui:true,guiInstall:true});try {
    const p=f.page;
    await p.locator('[data-tool="workbuddy"] .tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'原生安装器'}).waitFor();
    assert.equal((await p.locator('.tool-install-status').innerText()).split('请完成原生安装器中的步骤').length,2);
    assert.equal(await p.locator('.tool-install-cancel').isVisible(),false);
    assert.equal(await p.locator('.tool-install-stop-wait').isVisible(),true);
    await p.locator('.tool-install-stop-wait').click();
    await p.locator('.tool-install-status').filter({hasText:'已停止等待'}).waitFor();
    assert.match(await p.locator('.tool-install-status').innerText(),/请在原生安装器中取消/);
    assert.equal(await p.locator('.tool-install-retry').isVisible(),true);
    await p.locator('.tool-install-close').click();
    await p.locator('[data-tool="deepseek-harness"] .tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'原生安装器'}).waitFor();
    await p.locator('.tool-install-close').click();
    await p.evaluate(()=>{window.__fixture.nativeInstalled=true;});
    await p.locator('[data-tool="deepseek-harness"] .tool-connect').waitFor();
    assert.equal((await calls(p,'cancel_tool_installation')).length,1);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('OpenCode installation uses the shared flow and leaves connection as a separate action', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    await p.locator('[data-tool="opencode"] .tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    assert.deepEqual((await calls(p,'start_tool_installation'))[0].args,{tool:'opencode'});
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await p.locator('.tool-install-close').click();
    await p.locator('[data-tool="opencode"] .tool-connect').waitFor();
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('installation failure preserves the error and offers a retry', async () => {
  const f=await openFixture({installFail:true}); try {
    const p=f.page;
    await p.locator('[data-tool="openclaw"] .tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'依赖下载失败'}).waitFor();
    assert.match(await p.locator('.tool-install-output').innerText(),/运行依赖/);
    await p.locator('.tool-install-retry').click();
    await p.locator('.tool-install-status').filter({hasText:'依赖下载失败'}).waitFor();
    assert.equal((await calls(p,'start_tool_installation')).length,2);
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('real UI renders safe native summaries, searches, navigates and fits desktop/mobile', async () => {
  const f = await openFixture();try {
    const p=f.page;assert.equal(await p.locator('.tool-row').count(),14);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.equal((await calls(p,'connect_tool')).length,0);
    await p.locator('.tools-search').fill('Codex');assert.equal(await p.locator('.tool-row').count(),1);
    await p.locator('.tools-search').fill('no-match');await p.getByRole('heading',{name:'没有匹配的工具'}).waitFor();
    await p.getByRole('button',{name:'查看全部工具'}).click();assert.equal(await p.locator('.tool-row').count(),14);
    await p.locator('[data-tool-filter="available"]').click();assert.equal(await p.locator('.tool-row').count(),1);
    await p.locator('[data-tool-filter="all"]').click();
    for(const width of [1440,900,375]) { await p.setViewportSize({width,height:width===1440?1000:760});assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);assert.equal(await p.locator('#signed-in').isVisible(),true); }
    await detail(p,'claude-code');
    await p.getByRole('heading',{name:'已配置模型',exact:true}).waitFor();
    assert.equal(await p.getByRole('heading',{name:'配置文件',exact:true}).count(),0);
    assert.equal(await p.getByText('查看接入信息',{exact:true}).count(),0);
    assert.equal(await p.locator('.configuration-editor').count(),0);
    assert.equal((await calls(p,'tool_configuration_files')).length,0);
    await p.locator('.tool-detail-close').click();
    await p.locator('.primary-nav [data-page="account"]').click();await p.getByRole('heading',{name:'可切换空间'}).waitFor();
    await p.locator('.primary-nav [data-page="help"]').click();await p.getByRole('heading',{name:'常见问题'}).waitFor();assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});
test('new connect requires explicit confirmation, uses selected workspace and server model', async () => {
  const f=await openFixture({model:'model-from-catalogue'});try {
    const p=f.page;await switchTeam(p);assert.match(await p.locator('[data-tool="claude-code"]').innerText(),/个人空间/);
    await p.locator('[data-tool="workbuddy"] .tool-connect').click();await p.locator('.connect-confirm-dialog[open]').waitFor();
    assert.match(await p.locator('.connect-target').innerText(),/团队空间/);assert.equal((await calls(p,'connect_tool')).length,0);
    await p.locator('.connect-cancel').click();assert.equal((await calls(p,'connect_tool')).length,0);
    await p.locator('[data-tool="workbuddy"] .tool-connect').click();await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'WorkBuddy 已接入'}).waitFor();await p.locator('dialog[open] .close').click();
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'workbuddy',recovery:null});
    await detail(p,'workbuddy');assert.match(await p.locator('#tool-detail-region').innerText(),/团队空间/);assert.match(await p.locator('#tool-detail-region').innerText(),/model-from-catalogue/);
    assert.equal((await calls(p,'test_tool_connection')).length,0);await p.locator('#tool-detail-region .tool-launch').click();assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'workbuddy');assert.deepEqual(f.errors,[]);
  } finally {await f.close();}
});
test('paid test requires model and confirmation and keeps saved key/workspace after switch', async () => {
  const f=await openFixture({multi:true});try {
    const p=f.page;await switchTeam(p);await detail(p,'workbuddy');await p.locator('.tool-test').click();
    assert.match(await p.locator('.test-context').innerText(),/计费空间：个人空间/);
    assert.equal(await p.locator('.test-confirm').isDisabled(),true);assert.equal((await calls(p,'test_tool_connection')).length,0);
    await p.locator('.test-model').selectOption('server-model-b');await p.locator('.test-confirm').click();await p.locator('.test-feedback.success').waitFor();
    assert.deepEqual((await calls(p,'test_tool_connection'))[0].args,{tool:'workbuddy',expectedKeyId:'receipt-workbuddy',request:{confirmed:true,model:'server-model-b'}});
    await p.locator('.test-cancel').click();await p.evaluate(()=>window.__fixture.testPassed=false);await p.locator('.tool-test').click();await p.locator('.test-model').selectOption('server-model-a');await p.locator('.test-confirm').click();await p.locator('.test-feedback.error').waitFor();assert.match(await p.locator('.test-feedback').innerText(),/失败/);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('configuration drift disables testing until configuration is repaired', async () => {
  const f=await openFixture({changed:true});try {
    const p=f.page;await detail(p,'codex');assert.equal(await p.locator('.tool-test').isDisabled(),true);
    assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('connection test outcomes and saved history identify selected-model protocol scope', async () => {
  const f=await openFixture({multi:true});try {
    const p=f.page;
    await detail(p,'workbuddy');
    for(const passed of [true,false]) {
      await p.evaluate(passed=>{window.__fixture.testPassed=passed;},passed);
      await p.locator('.tool-test').click();
      assert.match(await p.locator('.test-scope').innerText(),/所选模型.*非流式接口文本响应/);
      assert.match(await p.locator('.test-scope').innerText(),/其他模型、流式、工具调用或 Agent 运行表现/);
      await p.locator('.test-model').selectOption('server-model-b');
      await p.locator('.test-confirm').click();
      await p.locator(`.test-feedback.${passed?'success':'error'}`).waitFor();
      assert.match(await p.locator('.test-feedback').innerText(),passed?/接口文本测试通过/:/接口文本测试失败/);
      await p.locator('.test-cancel').click();
      assert.match(await p.locator('#tool-detail-region .notice').innerText(),/其他模型、流式、工具调用或 Agent 运行表现/);
      assert.match(await p.locator('.test-history-scope').innerText(),/其他模型、流式、工具调用或 Agent 运行表现/);
    }
    assert.equal((await calls(p,'test_tool_connection')).length,2);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('reconnect preserves native receipt and expected current ownership in command arguments', async () => {
  const f=await openFixture();try {
    const p=f.page;await switchTeam(p);await detail(p,'claude-code');await manage(p);await p.locator('#tool-detail-region .tool-connect').click();
    assert.equal((await calls(p,'reconnect_tool')).length,0);assert.equal(await p.locator('.reconnect-dialog input[type=checkbox]').count(),0);assert.match(await p.locator('.reconnect-from').innerText(),/个人空间/);assert.match(await p.locator('.reconnect-to').innerText(),/团队空间/);
    await p.locator('.reconnect-confirm').click();await p.getByRole('heading',{name:'Claude Code 已接入'}).waitFor();
    assert.deepEqual((await calls(p,'reconnect_tool'))[0].args,{tool:'claude-code',expectedKeyId:'receipt-claude-code',expectedOwnerId:'user-fixture',expectedWorkspaceId:'team',recovery:null});assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('disconnect confirmation and conflict resolution forward only explicit receipt choices', async () => {
  const f=await openFixture({conflicts:true});try {
    const p=f.page;await detail(p,'claude-code');await manage(p);await p.locator('.tool-disconnect').click();assert.equal((await calls(p,'disconnect_tool')).length,0);
    await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入'}).click();await p.locator('.disconnect-dialog[open]').waitFor();assert.equal(await p.locator('.disconnect-confirm').isDisabled(),true);
    await p.locator('.disconnect-cancel').click();assert.equal((await calls(p,'disconnect_tool')).length,1);
    await manage(p);await p.locator('.tool-disconnect').click();await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入'}).click();await p.locator('.disconnect-conflicts select').selectOption('keep_current');await p.locator('.disconnect-confirm').click();await p.getByRole('heading',{name:'取消接入完成'}).waitFor();
    assert.deepEqual((await calls(p,'disconnect_tool')).at(-1).args,{tool:'claude-code',expectedKeyId:'receipt-claude-code',request:{revision:'receipt-revision',resolutions:[{file:0,field:1,choice:'keep_current'}]}});assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('native login waiting/cancel and logout preserve tool configuration, late inventory stays cleared', async () => {
  const f=await openFixture({signedIn:false});try {
    const p=f.page;await p.locator('#begin').click();await p.locator('#waiting').waitFor();assert.equal((await calls(p,'begin_desktop_login')).length,1);
    await p.locator('#cancel').click();await p.locator('#begin').waitFor();
    await p.evaluate(()=>{window.__fixture.signedIn=true;window.__fixture.emit('desktop-login-status',window.__fixture.loginStatus());});await p.locator('.tool-row').first().waitFor();
    await p.evaluate(()=>{window.__fixture.deferInventory=true;void window.TokenMarketTools.refresh();});
    await p.locator('.workspace-profile-button').click();await p.locator('.workspace-dialog[open]').getByRole('button',{name:'退出登录',exact:true}).click();await p.locator('#login').waitFor();await p.evaluate(()=>window.__fixture.resolveInventory());await p.waitForTimeout(50);
    assert.equal(await p.locator('.tool-row').count(),0);assert.equal((await calls(p,'disconnect_tool')).length,0);assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('permissions and unavailable balance are respected; workspace failure keeps logout accessible', async () => {
  for(const options of [{restricted:true,noBalance:true},{workspaceError:true}]){
    const f=await openFixture(options);try {const p=f.page;
      if(options.restricted){await p.locator('.primary-nav [data-page="account"]').click();assert.equal(await p.getByRole('button',{name:'Token 管理',exact:true}).count(),0);assert.equal(await p.getByRole('button',{name:'充值',exact:true}).count(),0);assert.match(await p.locator('.account-balance').innerText(),/余额暂不可用/);}
      else{await p.locator('.workspace-profile-button').click();await p.locator('.workspace-dialog[open]').getByRole('button',{name:'退出登录',exact:true}).click();await p.locator('#login').waitFor();}assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});
test('a refreshed workspace invalidates a pending connect before any configuration write', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('[data-tool="workbuddy"] .tool-connect').click();await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.evaluate(async()=>{window.__fixture.current=window.__fixture.team;await window.TokenMarketWorkspace.refresh();});
    assert.equal(await p.locator('.connect-confirm-dialog').isVisible(),false);assert.equal((await calls(p,'connect_tool')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('normal connection dialogs keep credential reuse and creation invisible', async () => {
  const f=await openFixture();try {
    const p=f.page;await detail(p,'claude-code');await manage(p);await p.locator('#tool-detail-region .tool-connect').click();
    assert.equal(await p.locator('.reconnect-dialog input[type=checkbox]').count(),0);
    assert.doesNotMatch(await p.locator('.reconnect-dialog').innerText(),/Key|凭据/);
    await p.locator('.reconnect-cancel').click();
    await p.locator('.primary-nav [data-page="tools"]').click();await p.locator('[data-tool="workbuddy"] .tool-connect').click();
    assert.equal(await p.locator('.connect-confirm-dialog input[type=checkbox]').count(),0);
    assert.doesNotMatch(await p.locator('.connect-confirm-dialog').innerText(),/Key|凭据/);
    await p.locator('.connect-confirm').click();await p.getByRole('heading',{name:'WorkBuddy 已接入'}).waitFor();
    assert.doesNotMatch(await p.locator('dialog[open]').innerText(),/Key|凭据/);
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'workbuddy',recovery:null});assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('missing local credentials require scoped recovery confirmation and no clear action exists', async () => {
  const f=await openFixture({credentialLost:true});try {
    const p=f.page;await detail(p,'claude-code');await manage(p);assert.equal(await p.getByRole('button',{name:'清除连接',exact:true}).count(),0);
    await p.locator('.tool-connect').filter({hasText:'重新接入'}).click();await p.locator('.reconnect-confirm').click();
    await p.locator('.recovery-dialog[open]').waitFor();assert.match(await p.locator('.recovery-scope').innerText(),/个人空间/);
    assert.equal((await calls(p,'reconnect_tool')).length,1);await p.locator('.recovery-cancel').click();assert.equal((await calls(p,'reconnect_tool')).length,1);
    await manage(p);await p.locator('.tool-connect').filter({hasText:'重新接入'}).click();await p.locator('.reconnect-confirm').click();await p.locator('.recovery-dialog[open]').waitFor();
    await p.locator('.recovery-confirm').click();await p.getByRole('heading',{name:'Claude Code 已接入'}).waitFor();
    const request=(await calls(p,'reconnect_tool')).at(-1).args;assert.deepEqual(request.recovery,{keyId:'missing-fixture',ownerId:'user-fixture',workspaceId:'personal',ownerName:'界面测试账号',workspaceName:'个人空间'});
    assert.deepEqual(f.errors,[]);
  } finally {await f.close();}
});

test('DuMate enterprise imports a catalogue with separate selection and DuMate check guidance', async () => {
  const f=await openFixture();try {
    const p=f.page; const row=p.locator('[data-tool="dumate-enterprise"]');
    await row.locator('img').evaluate(image=>image.decode());
    await row.locator('.tool-install').click();
    await p.locator('.tool-install-status').filter({hasText:'安装完成'}).waitFor();
    await p.locator('.tool-install-close').click();
    await detail(p,'dumate-enterprise');
    assert.match(await p.locator('#tool-detail-region').innerText(),/企业管理员/);
    await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.connect-confirm-dialog[open] .connect-confirm').click();
    await p.getByRole('heading',{name:'百度搭子 DuMate（企业版） 已接入'}).waitFor();
    assert.match(await p.locator('.restart-notice').innerText(),/导入 2 个/);
    assert.match(await p.locator('.restart-notice').innerText(),/保存该模型的真实检查状态/);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'dumate-enterprise',recovery:null});
    await p.locator('dialog[open] .close').click();
    await p.locator('#tool-detail-region .tool-test').click();
    const selector=p.locator('.connection-test-dialog select');
    assert.match(await p.locator('.connection-test-dialog .test-scope').innerText(),/写回 DuMate/);
    assert.equal(await selector.isDisabled(),false);
    assert.equal(await selector.locator('option').count(),3);
    assert.deepEqual(f.errors,[]);
  }finally {await f.close();}
});

test('one update action detects installs and versions on startup, explicit check and native reopen', async () => {
  const f=await openFixture();try {
    await f.page.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='check_tool_updates'));
    await f.page.getByRole('button',{name:'检查更新',exact:true}).waitFor();
    assert.equal((await calls(f.page,'check_tool_updates')).length,1);
    assert.equal(await f.page.getByRole('button',{name:'重新检测',exact:true}).count(),0);
    const before=(await calls(f.page,'desktop_tool_inventory')).length;
    await f.page.locator('#detect-tools').click();
    await f.page.waitForFunction(()=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length===2);
    assert.ok((await calls(f.page,'desktop_tool_inventory')).length>before);
    assert.equal(await f.page.locator('dialog[open]').count(),0);
    await f.page.evaluate(()=>window.dispatchEvent(new Event('focus')));
    await f.page.waitForTimeout(50);assert.equal((await calls(f.page,'check_tool_updates')).length,2);
    await f.page.evaluate(()=>window.__fixture.emit('desktop-check-tool-updates'));
    await f.page.waitForFunction(()=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length===3);
    assert.match(await f.page.locator('.tools-updates-status').textContent(),/3 个已安装工具/);
    assert.deepEqual(f.errors,[]);
  } finally {await f.close();}
});
test('latest release upgrade has current and target versions, explicit confirmation and preserves the connection', async () => {
  const f=await openFixture({upgradeTool:'codex',updateVersions:{codex:'0.150.0'}});try {
    const row=f.page.locator('[data-tool="codex"]');await row.locator('.install-column .tool-upgrade').waitFor();
    assert.equal(await row.locator('.tool-actions .tool-upgrade').count(),0);
    await row.locator('.tool-upgrade').click();assert.equal((await calls(f.page,'start_tool_upgrade')).length,0);
    assert.match(await f.page.locator('.tool-upgrade-confirm').textContent(),/0\.150\.0 → 0\.166\.0/);
    await f.page.locator('.tool-upgrade-confirm').getByRole('button',{name:'取消',exact:true}).click();
    assert.equal((await calls(f.page,'start_tool_upgrade')).length,0);
    await row.locator('.tool-upgrade').click();
    await f.page.locator('.tool-upgrade-confirm').getByRole('button',{name:'确认升级'}).click();
    await f.page.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');
    await f.page.locator('.tool-install-close').click();
    await f.page.waitForFunction(()=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length===2);
    assert.equal((await calls(f.page,'start_tool_upgrade')).length,1);
    for(const command of ['start_tool_installation','connect_tool','reconnect_tool','disconnect_tool','test_tool_connection'])assert.equal((await calls(f.page,command)).length,0);
    assert.equal(await row.locator('.tool-upgrade').count(),0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('official latest releases are directly upgradeable without a compatibility pin', async () => {
  const f=await openFixture({newerRelease:'codex',updateFailure:'workbuddy'});try {
    const p=f.page;await p.waitForFunction(()=>document.querySelector('.tools-updates-status').textContent.includes('检查未完成'));
    const row=p.locator('[data-tool="codex"]');assert.match(await row.locator('.tool-version-target').textContent(),/99\.0\.0/);assert.equal(await row.locator('.tool-upgrade').isVisible(),true);
    await detail(p,'codex');assert.match(await p.locator('.tool-version-sheet').textContent(),/99\.0\.0/);assert.doesNotMatch(await p.locator('.tool-version-sheet').textContent(),/已验证兼容版|待验证/);
    assert.equal((await calls(p,'start_tool_upgrade')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('pending checks are coalesced and late update results cannot restore logged-out UI', async () => {
  const f=await openFixture({deferUpdates:true});try {
    await f.page.waitForFunction(()=>Boolean(window.__fixture.resolveUpdates));
    await f.page.evaluate(()=>{window.__fixture.emit('desktop-check-tool-updates');window.__fixture.emit('desktop-check-tool-updates');});
    assert.equal((await calls(f.page,'check_tool_updates')).length,1);
    assert.equal(await f.page.locator('#detect-tools').isDisabled(),true);
    await f.page.locator('.workspace-profile-button').click();await f.page.locator('#account-menu').getByRole('button',{name:'退出登录'}).click();
    await f.page.evaluate(()=>window.__fixture.resolveUpdates());await f.page.waitForTimeout(50);
    assert.equal(await f.page.locator('.tool-row').count(),0);assert.equal(await f.page.locator('dialog[open]').count(),0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('upgrade can be cancelled and retried through the shared progress dialog', async () => {
  const f=await openFixture({upgradeTool:'codex',updateVersions:{codex:'0.150.0'},installPending:true});try {
    await f.page.locator('[data-tool="codex"] .tool-upgrade').click();await f.page.getByRole('button',{name:'确认升级'}).click();
    await f.page.getByRole('button',{name:'取消升级'}).click();await f.page.getByRole('button',{name:'重试升级'}).waitFor();
    await f.page.getByRole('button',{name:'重试升级'}).click();await f.page.waitForFunction(()=>window.__fixture.calls.filter(c=>c.command==='start_tool_upgrade').length===2);
    assert.equal((await calls(f.page,'start_tool_installation')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});


test('installation column owns upgrade and original-channel guidance at desktop widths', async () => {
  const f=await openFixture({upgradeTool:'codex',updateVersions:{codex:'0.150.0'},manualUpgrade:'workbuddy'});try {
    for(const width of [1280,900]) {
      await f.page.setViewportSize({width,height:800});
      assert.equal(await f.page.locator('[data-tool="codex"] .install-column .tool-upgrade').isVisible(),true);
      assert.equal(await f.page.locator('[data-tool="codex"] .tool-actions .tool-upgrade').count(),0);
      assert.equal(await f.page.locator('[data-tool="workbuddy"] .install-column .tool-upgrade-guide').isVisible(),true);
      assert.equal(await f.page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
    }
    await f.page.locator('[data-tool="workbuddy"] .tool-upgrade-guide').click();
    assert.equal(await f.page.locator('.tool-version-sheet').isVisible(),true);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('reference version column pairs numeric versions and actions and owns missing-tool installation', async () => {
  const f=await openFixture({upgradeTool:'codex',updateVersions:{codex:'codex-cli 0.150.0'}});try {
    const row=f.page.locator('[data-tool="codex"]');await row.locator('.tool-upgrade').waitFor();
    assert.equal(await row.locator('.tool-version').textContent(),'0.150.0');assert.match(await row.locator('.tool-version-target').textContent(),/0\.166\.0/);
    const geometry=await row.locator('.install-column').evaluate(cell=>{const a=cell.querySelector('.tool-version-copy').getBoundingClientRect(),b=cell.querySelector('.tool-upgrade').getBoundingClientRect();return {alongside:b.left>=a.right,sameRow:Math.max(a.top,b.top)<Math.min(a.bottom,b.bottom)};});
    assert.deepEqual(geometry,{alongside:true,sameRow:true});
    assert.equal(await f.page.locator('[data-tool="openclaw"] .install-column .tool-install').count(),1);
    assert.equal(await f.page.locator('[data-tool="openclaw"] > .tool-actions .tool-install').count(),0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('upgrade all counts official latest targets, confirms once and upgrades them sequentially', async () => {
  const f=await openFixture({upgradeTools:['claude-code','codex'],updateVersions:{'claude-code':'2.1.300',codex:'0.150.0'},manualUpgrade:'workbuddy'});try {
    const p=f.page;await p.getByRole('button',{name:'全部升级（2）',exact:true}).waitFor();
    await p.locator('#upgrade-all-tools').click();assert.equal((await calls(p,'start_tool_upgrade')).length,0);
    assert.equal(await p.locator('.batch-upgrade-row').count(),2);
    await p.getByRole('button',{name:'取消',exact:true}).click();assert.equal((await calls(p,'start_tool_upgrade')).length,0);
    await p.locator('#upgrade-all-tools').click();await p.getByRole('button',{name:'确认全部升级'}).click();
    await p.locator('.batch-upgrade-summary').filter({hasText:'已升级 2 个 · 未完成 0 个'}).waitFor();
    assert.deepEqual((await calls(p,'start_tool_upgrade')).map(call=>call.args.tool),['claude-code','codex']);
    const commands=await p.evaluate(()=>window.__fixture.calls.filter(c=>['start_tool_upgrade','tool_installation_status'].includes(c.command)));
    const second=commands.findIndex(c=>c.command==='start_tool_upgrade'&&c.args.tool==='codex');assert.ok(commands.slice(0,second).some(c=>c.command==='tool_installation_status'&&c.args.tool==='claude-code'));
    for(const cmd of ['start_tool_installation','connect_tool','reconnect_tool','disconnect_tool','test_tool_connection'])assert.equal((await calls(p,cmd)).length,0);
    await p.getByRole('button',{name:'完成',exact:true}).click();assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('upgrade all continues after a tool failure and reports individual outcomes', async () => {
  const f=await openFixture({upgradeTools:['claude-code','codex'],updateVersions:{'claude-code':'2.1.300',codex:'0.150.0'},upgradeFailTool:'claude-code'});try {
    const p=f.page;await p.getByRole('button',{name:'全部升级（2）',exact:true}).click();await p.getByRole('button',{name:'确认全部升级'}).click();
    await p.locator('.batch-upgrade-summary').filter({hasText:'已升级 1 个 · 未完成 1 个'}).waitFor();
    assert.equal((await calls(p,'start_tool_upgrade')).length,2);assert.match(await p.locator('.batch-upgrade-row').first().textContent(),/升级失败/);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('stopping upgrade all cancels the active job and skips the remaining queue', async () => {
  const f=await openFixture({upgradeTools:['claude-code','codex'],updateVersions:{'claude-code':'2.1.300',codex:'0.150.0'},installPending:true});try {
    const p=f.page;await p.getByRole('button',{name:'全部升级（2）',exact:true}).click();await p.getByRole('button',{name:'确认全部升级'}).click();
    await p.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='start_tool_upgrade'));
    await p.getByRole('button',{name:'停止全部升级'}).click();
    await p.locator('.batch-upgrade-summary').filter({hasText:'未执行 1 个'}).waitFor();
    assert.equal((await calls(p,'start_tool_upgrade')).length,1);assert.equal((await calls(p,'cancel_tool_installation')).length,1);assert.match(await p.locator('.batch-upgrade-row').last().textContent(),/未执行/);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('logging out during batch progress prevents later tools from starting', async () => {
  const f=await openFixture({upgradeTools:['claude-code','codex'],updateVersions:{'claude-code':'2.1.300',codex:'0.150.0'},installPending:true});try {
    const p=f.page;await p.getByRole('button',{name:'全部升级（2）',exact:true}).click();await p.getByRole('button',{name:'确认全部升级'}).click();
    await p.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='start_tool_upgrade'));
    await p.evaluate(()=>window.TokenMarketTools.setAuthenticated(false));await p.waitForTimeout(400);
    assert.equal((await calls(p,'start_tool_upgrade')).length,1);assert.equal(await p.locator('dialog[open]').count(),0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});


test('batch upgrades count shared DuMate personal and enterprise installations once', async () => {
  const f=await openFixture({installedTools:['dumate','dumate-enterprise'],upgradeTools:['dumate','dumate-enterprise'],updateVersions:{dumate:'1.0.88','dumate-enterprise':'1.0.88'}});try {
    await f.page.getByRole('button',{name:'全部升级（1）',exact:true}).click();
    assert.equal(await f.page.locator('.batch-upgrade-row').count(),1);
    assert.equal((await calls(f.page,'start_tool_upgrade')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('tool list keeps installation and connection separate without duplicated missing-state labels', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.getByRole('button',{name:'检查更新',exact:true}).waitFor();
    const missing=p.locator('[data-tool="openclaw"]');
    assert.equal(((await missing.textContent()).match(/未安装/g)||[]).length,1);
    assert.equal(await missing.locator('.tool-state').textContent(),'—');assert.equal(await missing.locator('.tool-state').getAttribute('title'),'安装后可接入');
    assert.equal(await p.locator('[data-tool="workbuddy"] .tool-state').textContent(),'未接入');
    assert.equal(await p.locator('.tools-group').count(),0);
    assert.equal(await p.locator('.tools-list > .tool-row').count(),14);
    assert.equal(await p.locator('[data-tool="openclaw"] .tool-model-summary').count(),0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('manual update clearly identifies the original channel and opens latest-version guidance without upgrading', async () => {
  const f=await openFixture({manualUpgrade:'codex',updateVersions:{codex:'0.150.0'}});try {
    const p=f.page;await p.locator('[data-tool="codex"]').getByRole('button',{name:'手动更新',exact:true}).click();
    const guide=p.locator('.tool-manual-update');assert.equal(await guide.isVisible(),true);
    assert.match(await guide.textContent(),/原安装渠道/);assert.match(await guide.textContent(),/目标版本 0\.166\.0/);
    assert.equal(await guide.evaluate(element=>element===document.activeElement),true);
    await guide.getByRole('button',{name:'打开官方更新指引'}).click();assert.equal((await calls(p,'download_tool')).length,1);
    for(const cmd of ['start_tool_upgrade','start_tool_installation','connect_tool','test_tool_connection'])assert.equal((await calls(p,cmd)).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('missing application preserves its saved connection while installation remains independently available', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.getByRole('button',{name:'检查更新',exact:true}).waitFor();
    await p.evaluate(async()=>{window.__fixture.entries.find(e=>e.id==='codex').installed=false;await window.TokenMarketTools.refresh(true);});
    const row=p.locator('[data-tool="codex"]');assert.equal(await row.locator('.tool-version-empty').textContent(),'未安装');
    assert.match(await row.locator('.tool-state').textContent(),/已配置，未测试/);assert.equal(await row.locator('.tool-install').isEnabled(),true);
    assert.equal(await row.locator('.tool-launch').isDisabled(),true);assert.equal((await calls(p,'disconnect_tool')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('current-to-latest upgrade passes the exact official target and removes fixed-version certification language', async () => {
  const f=await openFixture({upgradeTools:['claude-code','codex'],updateVersions:{'claude-code':'2.1.311',codex:'0.163.0'},latestVersions:{'claude-code':'2.1.318',codex:'0.166.0'}});try {
    const p=f.page;await p.getByRole('button',{name:'全部升级（2）',exact:true}).waitFor();
    const row=p.locator('[data-tool="claude-code"]');assert.match(await row.locator('.tool-version-target').textContent(),/2\.1\.318/);
    await row.locator('.tool-upgrade').click();assert.match(await p.locator('.tool-upgrade-confirm').textContent(),/2\.1\.311 → 2\.1\.318/);
    await p.getByRole('button',{name:'确认升级',exact:true}).click();await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');
    assert.deepEqual((await calls(p,'start_tool_upgrade'))[0].args,{tool:'claude-code',targetVersion:'2.1.318'});
    await p.locator('.tool-install-close').click();await detail(p,'claude-code');
    assert.doesNotMatch(await p.locator('.tool-version-sheet').textContent(),/已验证兼容版|内置安装版本|待验证/);
    assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('installed tools show their method and selected path and expand additional installations without side effects', async () => {
  const sources=[{path:'/Users/fixture/.npm-global/bin/codex',resolvedPath:'/Users/fixture/.npm-global/lib/node_modules/@openai/codex/bin/codex.js',method:'npm',selected:true},{path:'/Users/fixture/.local/bin/codex',resolvedPath:'/Users/fixture/.codex/packages/standalone/releases/0.145.0/bin/codex',method:'原生程序',selected:false}];
  const f=await openFixture({installations:{codex:sources}});try {
    const p=f.page,row=p.locator('[data-tool="codex"]');await p.getByRole('button',{name:'检查更新',exact:true}).waitFor();
    assert.equal(await row.locator('.tool-location-path').textContent(),sources[0].path);assert.equal(await row.locator('.tool-install-method').textContent(),'npm');
    const before=(await calls(p,'check_tool_updates')).length;
    await row.getByRole('button',{name:'另有 1 处安装'}).click();
    assert.equal(await row.locator('.tool-installations-toggle').getAttribute('aria-expanded'),'true');
    assert.equal(await row.locator('.tool-installation-location').count(),2);assert.equal(await row.locator('.tool-current-installation').count(),1);
    assert.match(await row.locator('.tool-installation-list').textContent(),/\.local\/bin\/codex/);
    await p.evaluate(()=>window.TokenMarketTools.refresh(true));assert.equal(await row.locator('.tool-installation-list').isVisible(),true);
    assert.equal((await calls(p,'check_tool_updates')).length,before);
    await row.getByRole('button',{name:'另有 1 处安装'}).click();assert.equal(await row.locator('.tool-installation-list').count(),0);
    for(const cmd of ['start_tool_installation','start_tool_upgrade','connect_tool','test_tool_connection'])assert.equal((await calls(p,cmd)).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('uninstalled tools describe the real installer and details distinguish program, config directory and config file', async () => {
  const f=await openFixture({configPaths:{codex:'/Users/fixture/custom config/codex/config.toml'}});try {
    const p=f.page;
    assert.match(await p.locator('[data-tool="openclaw"] .tool-planned-method').textContent(),/官方脚本和 npm/);
    assert.match(await p.locator('[data-tool="pi"] .tool-planned-method').textContent(),/官方独立安装包/);
    await detail(p,'codex');const info=p.locator('.tool-files-sheet');
    assert.match(await info.textContent(),/安装方式npm/);assert.match(await info.textContent(),/当前入口\/Users\/fixture\/\.npm-global\/bin\/codex/);
    assert.equal(await info.locator('.tool-path-row').filter({hasText:'配置目录'}).locator('code').textContent(),'/Users/fixture/custom config/codex');
    assert.equal(await info.locator('.tool-path-row').filter({hasText:'接入配置文件'}).locator('code').textContent(),'/Users/fixture/custom config/codex/config.toml');
    assert.match(await info.textContent(),/尚未生成/);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('installation paths render as selectable text and cannot inject page markup', async () => {
  const path='/Users/fixture/<img src=x onerror=alert(1)>/codex';
  const f=await openFixture({installations:{codex:[{path,method:'命令行程序',selected:true}]}});try {
    const row=f.page.locator('[data-tool="codex"]');assert.equal(await row.locator('.tool-location-path').textContent(),path);
    assert.equal(await row.locator('img').count(),1);await detail(f.page,'codex');
    assert.equal(await f.page.locator('.tool-files-sheet .tool-path-row').filter({hasText:'当前入口'}).locator('code').textContent(),path);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
