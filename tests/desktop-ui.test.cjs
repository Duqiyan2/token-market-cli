// Run with Node + Playwright installed. All native IPC is isolated in the page;
// these tests never load credentials, change tool files, launch tools or bill a model.
const {test, before, after} = require('node:test');
const assert = require('node:assert/strict');
const {chromium, webkit} = require('playwright');
let browser;
before(async () => { browser = await (process.env.TOKEN_MARKET_TEST_BROWSER_ENGINE === 'webkit' ? webkit : chromium).launch({headless:true, ...(process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL ? {channel:process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL} : {})}); });
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

test('signed-out Client Update downloads only after confirmation and leaves verified installation disabled', async () => {
  const f=await openFixture({signedIn:false,clientUpdate:{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',releaseNotes:'保留已有连接',error:null}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'下载客户端更新',exact:true}).click();
    await p.locator('dialog[open]').filter({hasText:'确认下载客户端更新'}).waitFor();
    assert.equal((await calls(p,'download_client_update')).length,0);
    await p.getByRole('button',{name:'取消下载',exact:true}).click();
    assert.equal((await calls(p,'download_client_update')).length,0);
    await p.getByRole('button',{name:'下载客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'确认下载',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'签名验证通过'}).waitFor();
    assert.deepEqual((await calls(p,'download_client_update')).map(c=>c.args),[{version:'1.1.0'}]);
    assert.equal(await p.getByRole('button',{name:'安装并重启',exact:true}).isDisabled(),true);
    assert.match(await p.locator('#client-install-unavailable').textContent(),/暂不可用/);
    assert.equal((await p.evaluate(()=>window.__fixture.calls.filter(c=>/^(install|restart).*client|client.*(install|restart)/.test(c.command)))).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('Client Update installation explains active work and requires an explicit retry before guarded handoff', async () => {
  const f=await openFixture({signedIn:false,startupClientUpdate:{status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true},clientInstallBusy:true});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.getByRole('button',{name:'安装并重启',exact:true}).click();
    await p.getByRole('button',{name:'取消安装',exact:true}).click();
    assert.equal((await calls(p,'install_client_update')).length,0);
    await p.getByRole('button',{name:'安装并重启',exact:true}).click();
    await p.getByRole('button',{name:'确认安装并重启',exact:true}).click();
    await p.locator('#client-install-error').filter({hasText:'工具连接操作正在进行'}).waitFor();
    assert.equal(await p.getByRole('button',{name:'安装并重启',exact:true}).isEnabled(),true);
    assert.deepEqual((await calls(p,'install_client_update')).map(c=>c.args),[{version:'1.1.0'}]);
    await p.evaluate(()=>window.__fixture.clientInstallBusy=false);
    await p.getByRole('button',{name:'安装并重启',exact:true}).click();
    await p.getByRole('button',{name:'确认安装并重启',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    assert.doesNotMatch(await p.locator('#client-update-status').textContent(),/更新完成|安装完成/);
    assert.equal((await calls(p,'install_client_update')).length,2);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('Windows Client Update enables installation after a verified download and hands off only after confirmation', async () => {
  const f=await openFixture({signedIn:false,deferClientDownload:true,clientUpdate:{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#check-client-update').click();
    await p.getByRole('button',{name:'下载客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'确认下载',exact:true}).click();
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveClientDownload));
    assert.equal(await p.locator('#install-client-update').isVisible(),false);
    await p.evaluate(()=>window.__fixture.resolveClientDownload({status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true}));
    const install=p.getByRole('button',{name:'安装并重启',exact:true});
    await install.waitFor();assert.equal(await install.isEnabled(),true);
    assert.equal(await p.locator('#client-install-unavailable').isVisible(),false);
    await install.click();
    assert.equal((await calls(p,'install_client_update')).length,0);
    await p.getByRole('button',{name:'确认安装并重启',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    assert.deepEqual((await calls(p,'install_client_update')).map(c=>c.args),[{version:'1.1.0'}]);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('Windows Client Update displays its native installation rejection without macOS guidance', async () => {
  const reason='安装并重启暂不可用：无法确认当前用户 NSIS 安装来源，请先使用官方安装包手动更新';
  const f=await openFixture({signedIn:false,startupClientUpdate:{status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:false,installUnavailableReason:reason}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    assert.equal(await p.locator('#install-client-update').isDisabled(),true);
    assert.equal(await p.locator('#client-install-unavailable').innerText(),reason);
    assert.doesNotMatch(await p.locator('#client-install-unavailable').innerText(),/macOS|Apple Silicon|应用程序/);
    assert.equal((await calls(p,'install_client_update')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('pending Client Update startup shows recovery without claiming completion or enabling another install', async () => {
  const pending={status:'handoff',currentVersion:'1.0.0',error:'尚未确认目标客户端版本启动，请使用官方安装包恢复'};
  const f=await openFixture({signedIn:false,startupClientUpdate:pending,clientUpdate:pending});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#client-install-error').filter({hasText:'尚未确认目标客户端版本启动'}).waitFor();
    assert.doesNotMatch(await p.locator('#client-update-status').textContent(),/更新完成|安装完成/);
    assert.equal(await p.locator('#check-client-update').isEnabled(),true);
    await p.locator('#check-client-update').click();
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    assert.equal(await p.locator('#install-client-update').isVisible(),false);
    assert.equal((await calls(p,'install_client_update')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('explicit Client Update recovery clears handoff and rejects late pre-recovery events before a later intentional install', async () => {
  const pending={status:'handoff',currentVersion:'1.0.0',error:'工具连接操作正在进行，请完成后重试'};
  const f=await openFixture({signedIn:false,startupClientUpdate:pending});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    assert.equal(await p.locator('#check-client-update').isEnabled(),true);
    await p.locator('#check-client-update').click();
    await p.locator('#client-update-status').filter({hasText:'没有更新的受支持稳定版本'}).waitFor();
    await p.evaluate(value=>{
      window.__fixture.emit('desktop-client-update-status',value);
      window.__fixture.emit('desktop-client-update-status',{status:'verifying',currentVersion:'1.0.0',availableVersion:'1.1.0'});
      window.__fixture.emit('desktop-client-update-status',{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',automatic:true,remind:true});
    },pending);
    assert.match(await p.locator('#client-update-status').textContent(),/没有更新的受支持稳定版本/);
    assert.equal(await p.locator('#client-update-reminder').isVisible(),false);
    await p.evaluate(()=>{window.__fixture.clientUpdate={status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true};});
    await p.locator('#check-client-update').click();
    await p.getByRole('button',{name:'下载客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'确认下载',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'签名验证通过'}).waitFor();
    // The download fixture has no native executor by default; enable its reply.
    await p.evaluate(()=>window.__fixture.emit('desktop-client-update-status',{status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true}));
    await p.getByRole('button',{name:'安装并重启',exact:true}).click();
    await p.getByRole('button',{name:'确认安装并重启',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    assert.equal((await calls(p,'install_client_update')).length,1);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('a rejected current Client Update install restores retry despite old handoff events before and after its result', async () => {
  const pending={status:'handoff',currentVersion:'1.0.0',error:'上次客户端交接尚未确认'};
  const available={status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true};
  const f=await openFixture({signedIn:false,startupClientUpdate:pending,clientUpdate:available,deferClientDownload:true,deferClientInstall:true});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    await p.locator('#check-client-update').click();
    await p.getByRole('button',{name:'下载客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'确认下载',exact:true}).click();
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveClientDownload));
    await p.evaluate(()=>window.__fixture.resolveClientDownload({status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true}));
    await p.getByRole('button',{name:'安装并重启',exact:true}).click();
    await p.getByRole('button',{name:'确认安装并重启',exact:true}).click();
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveClientInstall));
    await p.evaluate(value=>window.__fixture.emit('desktop-client-update-status',value),pending);
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    await p.evaluate(()=>window.__fixture.resolveClientInstall({status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true,error:'工具连接操作正在进行，请完成后重试安装客户端更新'}));
    await p.locator('#client-install-error').filter({hasText:'工具连接操作正在进行'}).waitFor({timeout:5000});
    await p.evaluate(value=>window.__fixture.emit('desktop-client-update-status',value),pending);
    assert.match(await p.locator('#client-update-status').textContent(),/签名验证通过/);
    assert.equal(await p.getByRole('button',{name:'安装并重启',exact:true}).isEnabled(),true);
    await p.getByRole('button',{name:'安装并重启',exact:true}).click();
    await p.getByRole('button',{name:'确认安装并重启',exact:true}).click();
    await p.waitForFunction(()=>window.__fixture.calls.filter(c=>c.command==='install_client_update').length===2);
    await p.evaluate(()=>window.__fixture.resolveClientInstall({status:'handoff',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true}));
    await p.locator('#client-update-status').filter({hasText:'安装交接已启动'}).waitFor();
    await p.evaluate(()=>window.__fixture.emit('desktop-client-update-status',{status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0',installAvailable:true}));
    assert.match(await p.locator('#client-update-status').textContent(),/安装交接已启动/);
    assert.equal(await p.locator('#install-client-update').isVisible(),false);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('Client Update omits the removed recovery entry regardless of native recovery configuration', async () => {
  for (const available of [false,true]) {
    const f=await openFixture({signedIn:false,startupClientUpdate:{status:'handoff',currentVersion:'1.0.0',error:'尚未确认目标客户端版本启动',recoveryAvailable:available}});try {
      const p=f.page;await p.locator('#login [data-desktop-settings]').click();
      assert.equal(await p.getByRole('button',{name:'官方安装包与恢复说明',exact:true}).count(),0);
      assert.equal(await p.locator('#client-recovery-help').count(),0);
      assert.equal((await calls(p,'open_client_update_recovery')).length,0);
      assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});

test('known terminal Client Update failure remains visible with an explicit retry action', async () => {
  const f=await openFixture({signedIn:false,startupClientUpdate:{status:'install_failed',currentVersion:'1.0.0',error:'上次客户端安装失败，退出码 17',automatic:false,recoveryAvailable:true}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.locator('#client-update-status').filter({hasText:'退出码 17'}).waitFor();
    assert.equal(await p.getByRole('button',{name:'重试检查客户端更新',exact:true}).isEnabled(),true);
    assert.equal(await p.getByRole('button',{name:'官方安装包与恢复说明',exact:true}).count(),0);
    assert.equal(await p.locator('#client-update-reminder').isVisible(),false);
    assert.equal((await calls(p,'install_client_update')).length,0);
    await p.getByRole('button',{name:'重试检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'没有更新的受支持稳定版本'}).waitFor();
    assert.equal((await calls(p,'check_client_update')).length,1);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('terminal Client Update notice is nonmodal and acknowledged once for native process reloads', async () => {
  const value={status:'install_failed',currentVersion:'1.0.0',error:'上次客户端安装失败，退出码 17',automatic:false,recoveryAvailable:true,showRecoveryNotice:true};
  const f=await openFixture({signedIn:false,startupClientUpdate:value});try {
    const p=f.page;
    await p.locator('#client-update-failure-notice').filter({hasText:'退出码 17'}).waitFor({timeout:5000});
    assert.equal(await p.locator('dialog[open]').count(),0);
    await p.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='acknowledge_client_update_failure'));
    assert.equal((await calls(p,'acknowledge_client_update_failure')).length,1);
    await p.getByRole('button',{name:'知道了',exact:true}).click();
    await p.evaluate(v=>window.__fixture.emit('desktop-client-update-status',v),value);
    assert.equal(await p.locator('#client-update-failure-notice').isVisible(),false);
    assert.equal((await calls(p,'acknowledge_client_update_failure')).length,1);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('a late download progress event cannot erase a verified Client Update', async () => {
  const f=await openFixture({signedIn:false,clientUpdate:{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',releaseNotes:null,error:null}});try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'下载客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'确认下载',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'签名验证通过'}).waitFor();
    await p.evaluate(()=>window.__fixture.emit('desktop-client-update-status',{status:'verifying',currentVersion:'1.0.0',availableVersion:'1.1.0'}));
    await p.evaluate(()=>window.__fixture.emit('desktop-client-update-status',{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',automatic:true,remind:true}));
    assert.equal(await p.locator('#client-update-reminder').isVisible(),false);
    assert.match(await p.locator('#client-update-status').textContent(),/签名验证通过/);
    assert.equal(await p.getByRole('button',{name:'安装并重启',exact:true}).isVisible(),true);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('Client Update shows download and verification separately and retries a failed package without changing Tool Connections', async () => {
  const f=await openFixture({noBalance:true,deferClientDownload:true,clientUpdate:{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',releaseNotes:'稳定版本',error:null}});try {
    const p=f.page;await p.locator('#signed-in [data-desktop-settings]').click();
    const connections=await p.evaluate(()=>window.__fixture.entries.map(e=>e.connection));
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'下载客户端更新',exact:true}).click();
    await p.getByRole('button',{name:'确认下载',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'正在下载客户端更新'}).waitFor();
    assert.equal(await p.locator('#check-client-update').isDisabled(),true);
    await p.evaluate(()=>window.__fixture.emit('desktop-client-update-status',{status:'verifying',currentVersion:'1.0.0',availableVersion:'1.1.0'}));
    await p.locator('#client-update-status').filter({hasText:'正在验证更新包签名'}).waitFor();
    assert.equal(await p.locator('#install-client-update').isVisible(),false);
    await p.evaluate(()=>window.__fixture.resolveClientDownload({status:'failed',currentVersion:'1.0.0',availableVersion:'1.1.0',error:'签名无效，请重试下载'}));
    await p.locator('#client-update-status').filter({hasText:'签名无效'}).waitFor();
    await p.getByRole('button',{name:'重试下载客户端更新',exact:true}).click();
    assert.equal((await calls(p,'download_client_update')).length,1);
    await p.getByRole('button',{name:'确认下载',exact:true}).click();
    await p.evaluate(()=>window.__fixture.resolveClientDownload({status:'ready',currentVersion:'1.0.0',availableVersion:'1.1.0'}));
    await p.locator('#client-update-status').filter({hasText:'签名验证通过'}).waitFor();
    assert.equal((await calls(p,'download_client_update')).length,2);
    assert.deepEqual(await p.evaluate(()=>window.__fixture.entries.map(e=>e.connection)),connections);
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
    await p.locator('.dumate-model-dialog[open] .dumate-model-select:enabled').waitFor();
    const target=(await calls(p,'prepare_dumate_models'))[0].args.target;
    assert.equal(target.preserveBinding,false);
    assert.equal(target.expectedOwnerId,'user-fixture');
    assert.equal((await calls(p, 'test_tool_connection')).length, 0);
    assert.deepEqual(f.errors, []);
  } finally { await f.close(); }
});

test('DuMate personal installs separately, configures one model, and tests only on request', async () => {
  const f=await openFixture(); try {
    const p=f.page; const row=p.locator('[data-tool="dumate"]');
    await row.locator('img').evaluate(image=>image.decode());
    await row.locator('.tool-install').click();
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');

    assert.equal((await calls(p,'connect_tool')).length,0);
    await detail(p,'dumate');
    assert.match(await p.locator('#tool-detail-region').innerText(),/一次配置一个模型/);
    await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-dialog[open] .dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    assert.match(await p.locator('.restart-notice').innerText(),/已保存，重启搭子后生效/);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await p.locator('dialog[open] .close').click();
    await p.locator('#tool-detail-region .tool-test').click();
    const selector=p.locator('.connection-test-dialog select');
    assert.equal(await selector.isDisabled(),true);
    assert.equal(await selector.locator('option').count(),1);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'configure_dumate_model'))[0].args.model,'fixture-model');
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
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');

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
    await row.locator('.tool-install').click();await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');
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
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');

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
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');

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
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');

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
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');

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
    assert.equal(await p.locator('.tool-install-dialog[open]').count(),0);
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');
    assert.equal((await calls(p,'start_tool_installation')).length,1);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);

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
      await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');

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
    await detail(p,'openclaw');
    await p.locator('#tool-detail-region .tool-install-view').click();
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

test('GUI download opens the installer and restores Install until actual discovery succeeds', async () => {
  const f=await openFixture({missingGui:true,guiInstall:true});try {
    const p=f.page,row=p.locator('[data-tool="workbuddy"]');
    await row.locator('.tool-install').click();
    await p.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='start_tool_installation'));
    await row.locator('.tool-install:enabled').waitFor();
    assert.equal(await p.locator('dialog[open]').count(),0);
    assert.equal(await row.locator('.tool-connect').count(),0);
    await row.locator('.tool-install').click();
    await p.waitForFunction(()=>window.__fixture.calls.filter(c=>c.command==='start_tool_installation').length===2);
    await p.evaluate(()=>{const e=window.__fixture.entries.find(e=>e.id==='workbuddy');e.installed=true;e.canConnect=true;window.dispatchEvent(new Event('focus'));});
    await row.locator('.tool-connect').waitFor();
    assert.equal((await calls(p,'cancel_tool_installation')).length,0);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('QwenWork downloads from its enabled action and connects only after actual installation', async () => {
  const f=await openFixture({guiInstall:true});try {
    const p=f.page,row=p.locator('[data-tool="qwenwork"]');
    const button=row.getByRole('button',{name:'下载安装',exact:true});
    assert.equal(await button.isEnabled(),true);
    assert.doesNotMatch(await row.innerText(),/暂无安装方案|官网手动安装/);
    await button.click();
    await p.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='start_tool_installation'));
    await row.locator('.tool-install:enabled').waitFor();
    assert.deepEqual((await calls(p,'start_tool_installation'))[0].args,{tool:'qwenwork'});
    assert.equal(await row.locator('.tool-connect').count(),0);
    assert.equal(await p.locator('dialog[open]').count(),0);
    await button.click();
    await p.waitForFunction(()=>window.__fixture.calls.filter(c=>c.command==='start_tool_installation').length===2);
    await p.evaluate(()=>{const e=window.__fixture.entries.find(e=>e.id==='qwenwork');e.installed=true;e.canConnect=true;window.dispatchEvent(new Event('focus'));});
    await row.locator('.tool-connect').waitFor();
    assert.equal((await calls(p,'download_tool')).length,0);
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('QwenWork online updates reuse the background download flow without completion or reconnect popups', async () => {
  const f=await openFixture({installedTools:['qwenwork'],qwenworkOnlineUpdate:true});try {
    const p=f.page,row=p.locator('[data-tool="qwenwork"]');
    await row.locator('.tool-upgrade').waitFor();
    assert.match(await p.locator('#upgrade-all-tools').innerText(),/（0）/);
    await row.locator('.tool-upgrade').click();
    const confirmation=p.locator('.tool-upgrade-confirm[open]');
    assert.match(await confirmation.innerText(),/官方最新版/);
    assert.doesNotMatch(await confirmation.innerText(),/新版本 latest|重新接入|暂不支持/);
    await confirmation.getByRole('button',{name:'确认更新',exact:true}).click();
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='installer_opened');
    await row.locator('.tool-upgrade:enabled').waitFor();
    assert.equal(await p.locator('dialog[open]').count(),0);
    assert.doesNotMatch(await row.innerText(),/安装器已打开|已升级|重新接入|安装完成/);
    assert.equal((await calls(p,'start_tool_upgrade')).length,1);
    for(const command of ['connect_tool','reconnect_tool','test_tool_connection']) assert.equal((await calls(p,command)).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

for(const [state,label,message] of [
  ['reconnect_required','需要重新接入','工具安装已变化，请重新接入以更新适配'],
  ['unsupported','当前版本暂不支持','当前版本暂不支持接入，请联系管理员'],
]) test(`external tool version changes show ${state} on the existing connection`, async () => {
  const f=await openFixture({multi:true,connectionCompatibilities:{workbuddy:{state,message,keyId:'receipt-workbuddy',connectionId:'connection-workbuddy'}}});try {
    const p=f.page,row=p.locator('[data-tool="workbuddy"]');
    await row.locator('.tool-state').filter({hasText:label}).waitFor();
    assert.equal(await p.locator('dialog[open]').count(),0);
    await detail(p,'workbuddy');
    assert.match(await p.locator('#tool-detail-region').innerText(),new RegExp(message));
    assert.equal(await p.locator('#tool-detail-region .tool-test').isDisabled(),true);
    assert.equal((await calls(p,'reconnect_tool')).length,0);
    assert.equal((await calls(p,'start_tool_upgrade')).length,0);
    await p.evaluate(()=>{window.__fixture.entries.find(e=>e.id==='workbuddy').connection.connectionId='new-connection';window.dispatchEvent(new Event('focus'));});
    await p.locator('#tool-detail-region .tool-test:enabled').waitFor();
    assert.doesNotMatch(await p.locator('#tool-detail-region').innerText(),new RegExp(message));
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('unknown compatibility preserves the existing connection and actions', async () => {
  const f=await openFixture({multi:true,connectionCompatibilities:{workbuddy:{state:'unknown',message:'接入兼容性暂无法确认，请联系管理员',keyId:'receipt-workbuddy',connectionId:'connection-workbuddy'}}});try {
    const p=f.page,row=p.locator('[data-tool="workbuddy"]');
    await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    assert.doesNotMatch(await row.innerText(),/兼容性待确认|需要重新接入|当前版本暂不支持/);
    await detail(p,'workbuddy');
    await p.locator('#tool-detail-region .tool-test:enabled').waitFor();
    assert.doesNotMatch(await p.locator('#tool-detail-region').innerText(),/兼容性暂无法确认|联系管理员/);
    assert.equal(await p.locator('dialog[open]').count(),0);
    for(const command of ['connect_tool','reconnect_tool','test_tool_connection','start_tool_upgrade']) assert.equal((await calls(p,command)).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('OpenCode installation uses the shared flow and leaves connection as a separate action', async () => {
  const f=await openFixture(); try {
    const p=f.page;
    await p.locator('[data-tool="opencode"] .tool-install').click();
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');
    assert.deepEqual((await calls(p,'start_tool_installation'))[0].args,{tool:'opencode'});
    assert.equal((await calls(p,'connect_tool')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);

    await p.locator('[data-tool="opencode"] .tool-connect').waitFor();
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('installation failure preserves the error and offers a retry', async () => {
  const f=await openFixture({installFail:true}); try {
    const p=f.page;
    await p.locator('[data-tool="openclaw"] .tool-install').click();
    await detail(p,'openclaw');
    await p.locator('#tool-detail-region .tool-install-view').click();
    await p.locator('.tool-install-status').filter({hasText:'依赖下载失败'}).waitFor();
    assert.match(await p.locator('.tool-install-output').innerText(),/运行依赖/);
    await p.getByRole('button',{name:'查看其他安装方式',exact:true}).click();
    assert.deepEqual((await calls(p,'download_tool'))[0].args,{tool:'openclaw'});
    assert.equal((await calls(p,'connect_tool')).length,0);
    await p.locator('.tool-install-retry').click();
    await p.locator('.tool-install-status').filter({hasText:'依赖下载失败'}).waitFor();
    assert.equal((await calls(p,'start_tool_installation')).length,2);
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('idle dashboard never polls installation, workspace, versions or login renewal', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.clock.install();await p.reload();
    await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    const commands=['desktop_tool_inventory','desktop_workspace_context','check_tool_updates','refresh_desktop_session'];
    const before=await Promise.all(commands.map(command=>calls(p,command).then(value=>value.length)));
    await p.clock.fastForward(10*60*1000);
    assert.deepEqual(await Promise.all(commands.map(command=>calls(p,command).then(value=>value.length))),before);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('activation events coalesce and patch changed tool values without replacing rows or open controls', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    await p.locator('[data-tool="codex"] .tool-lifecycle-menu summary').click();
    const commands=['desktop_tool_inventory','desktop_workspace_context','check_tool_updates','refresh_desktop_session'];
    const before=await Promise.all(commands.map(command=>calls(p,command).then(value=>value.length)));
    await p.evaluate(()=>{
      window.__stableNodes=[document.querySelector('[data-tool="codex"]'),document.querySelector('[data-tool="claude-code"]'),document.querySelector('[data-tool="codex"] .tool-version'),document.querySelector('[data-tool="codex"] .tool-launch')];
      window.__fixture.updateVersions.codex='0.170.0';
      window.__fixture.entries.find(e=>e.id==='codex').connection.state='test_passed';
      window.dispatchEvent(new Event('focus'));document.dispatchEvent(new Event('visibilitychange'));window.__fixture.emit('desktop-activated');
    });
    await p.locator('[data-tool="codex"] .tool-version').filter({hasText:'0.170.0'}).waitFor();
    assert.deepEqual(await Promise.all(commands.map(command=>calls(p,command).then(value=>value.length))),before.map(value=>value+1));
    assert.equal(await p.evaluate(()=>window.__stableNodes.every(node=>node.isConnected)),true);
    assert.equal(await p.locator('[data-tool="codex"] .tool-lifecycle-menu').getAttribute('open'),'');
    assert.match(await p.locator('[data-tool="codex"] > .tool-state').innerText(),/测试通过/);
    await p.locator('[data-tool="codex"] .tool-uninstall').click();
    await p.locator('.tool-uninstall-confirm[open]').waitFor();
    assert.equal(await p.locator('[data-tool="codex"] .tool-lifecycle-menu').getAttribute('open'),null);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('account and balance updates retain their nodes, menu and keyboard focus', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('.workspace-balance').waitFor();await p.locator('.workspace-profile-button').click();
    await p.keyboard.press('ArrowDown');
    await p.evaluate(()=>{
      window.__balanceNode=document.querySelector('.workspace-balance');window.__accountNode=document.querySelector('.account-balance');window.__profileNode=document.querySelector('.workspace-profile-button');window.__menuFocus=document.activeElement;
      window.__fixture.balanceAmount='42.123';window.__fixture.user.display_name='更新后的账号';window.__fixture.emit('desktop-activated');
    });
    await p.locator('.workspace-balance').filter({hasText:'42.12'}).waitFor();
    assert.equal(await p.evaluate(()=>window.__balanceNode===document.querySelector('.workspace-balance')&&window.__accountNode===document.querySelector('.account-balance')&&window.__profileNode===document.querySelector('.workspace-profile-button')),true);
    assert.equal(await p.locator('#account-menu').isVisible(),true);
    assert.equal(await p.evaluate(()=>document.activeElement===window.__menuFocus),true);
    assert.match(await p.locator('#account-menu .profile-menu-identity').innerText(),/更新后的账号/);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('page and workspace changes synchronize information without an idle timer', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    const before=(await calls(p,'check_tool_updates')).length;
    await p.locator('.primary-nav [data-page="account"]').click();
    await p.waitForFunction(before=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length===before+1,before);
    await p.locator('.account-switch-action').click();
    await p.locator('dialog[open] [data-workspace-id="team"]').click();
    await p.waitForFunction(before=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length===before+2,before);
    assert.match(await p.locator('.account-current-copy').innerText(),/团队空间/);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('an open tool detail patches versions and models and its retained test button uses new data', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    await detail(p,'codex');
    await p.evaluate(()=>{
      window.__detailNodes=[document.querySelector('.tool-detail-heading'),document.querySelector('.tool-files-sheet'),document.querySelector('.tool-version-sheet'),document.querySelector('#tool-detail-region .tool-test')];
      window.__fixture.updateVersions.codex='0.171.0';window.__fixture.entries.find(e=>e.id==='codex').connection.models=['updated-model'];window.__fixture.emit('desktop-activated');
    });
    await p.locator('.tool-version-sheet').filter({hasText:'0.171.0'}).waitFor();
    assert.equal(await p.evaluate(()=>window.__detailNodes.every(node=>node.isConnected)),true);
    assert.equal(await p.locator('#tool-detail-region .tool-model-entry code').innerText(),'updated-model');
    await p.locator('#tool-detail-region .tool-test').click();
    await p.locator('.connection-test-dialog[open]').waitFor();
    assert.equal(await p.locator('.connection-test-dialog select option').last().getAttribute('value'),'updated-model');
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('activation during an in-flight sync is followed by one sync with the latest installation state', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    const before=(await calls(p,'check_tool_updates')).length;
    await p.evaluate(()=>{window.__fixture.deferUpdates=true;window.__fixture.emit('desktop-activated');});
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveUpdates));
    await p.evaluate(()=>{window.__fixture.entries.find(e=>e.id==='openclaw').installed=true;window.__fixture.emit('desktop-activated');window.dispatchEvent(new Event('focus'));});
    // Allow the debounced activation to arrive while the first request is pending.
    await p.waitForTimeout(150);
    assert.equal((await calls(p,'check_tool_updates')).length,before+1);
    await p.evaluate(()=>{window.__fixture.deferUpdates=false;window.__fixture.resolveUpdates();});
    await p.waitForFunction(before=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length===before+2,before);
    await p.locator('[data-tool-filter="uninstalled"]').filter({hasText:'未安装 10'}).waitFor();
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('login status uses native notifications and expiry deadlines instead of idle polling', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.clock.install();await p.reload();
    await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    const before=(await calls(p,'desktop_login_status')).length;
    await p.clock.fastForward(30000);
    assert.equal((await calls(p,'desktop_login_status')).length,before);
    await p.evaluate(()=>{window.__fixture.signedIn=false;window.__fixture.emit('desktop-login-status',window.__fixture.loginStatus());});
    assert.equal(await p.locator('#login').isVisible(),true);
    await p.evaluate(()=>{
      window.__fixture.pending={session_id:'expiry-fixture',authorize_url:'https://example.invalid',expires_at:new Date(Date.now()+5000).toISOString()};
      window.__fixture.emit('desktop-login-status',window.__fixture.loginStatus());
      window.__fixture.pending=null;
    });
    assert.equal(await p.locator('#waiting').isVisible(),true);
    await p.clock.fastForward(5100);
    assert.equal((await calls(p,'desktop_login_status')).length,before+1);
    assert.equal(await p.locator('#waiting').isVisible(),false);
    await p.evaluate(()=>{window.__fixture.signedIn=true;window.__fixture.emit('desktop-login-status',window.__fixture.loginStatus());});
    assert.equal(await p.locator('#signed-in').isVisible(),true);
    const after=(await calls(p,'desktop_login_status')).length;
    await p.clock.fastForward(30000);
    assert.equal((await calls(p,'desktop_login_status')).length,after);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('inventory refresh retains content while waiting and after failure, then displays real changes', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    await detail(p,'codex');
    await p.evaluate(()=>{
      window.__inventoryRow=document.querySelector('[data-tool="codex"]');
      window.__inventoryDetail=document.querySelector('#tool-detail-region').firstElementChild;
      window.__fixture.deferInventory=true;
      window.__inventoryRefresh=window.TokenMarketTools.refresh();
    });
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveInventory));
    assert.equal(await p.locator('.tool-group-heading').count(),2);
    assert.equal(await p.evaluate(()=>window.__inventoryRow===document.querySelector('[data-tool="codex"]')),true);
    assert.equal(await p.evaluate(()=>window.__inventoryDetail===document.querySelector('#tool-detail-region').firstElementChild),true);
    await p.evaluate(async()=>{window.__fixture.deferInventory=false;window.__fixture.resolveInventory();await window.__inventoryRefresh;});
    assert.equal(await p.evaluate(()=>window.__inventoryRow===document.querySelector('[data-tool="codex"]')),true);
    await p.evaluate(async()=>{window.__fixture.inventoryError=true;await window.TokenMarketTools.refresh();});
    assert.equal(await p.locator('.tool-row').count(),14);
    assert.equal(await p.evaluate(()=>window.__inventoryRow===document.querySelector('[data-tool="codex"]')),true);
    await p.evaluate(async()=>{window.__fixture.inventoryError=false;window.__fixture.entries.find(e=>e.id==='openclaw').installed=true;await window.TokenMarketTools.refresh();});
    assert.equal(await p.locator('[data-tool-filter="uninstalled"]').innerText(),'未安装 10');
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
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

test('DuMate enterprise stays hidden from tools, search and update counts', async () => {
  const f=await openFixture({installedTools:['dumate','dumate-enterprise'],upgradeTools:['dumate-enterprise']});try {
    const p=f.page;
    await p.getByRole('button',{name:'检查更新',exact:true}).waitFor();
    assert.equal(await p.locator('[data-tool="dumate-enterprise"]').count(),0);
    assert.equal(await p.locator('[data-tool="dumate"]').count(),1);
    assert.equal(await p.locator('.tools-count').innerText(),'显示 14 个工具，共 14 个');
    assert.match(await p.locator('.tools-updates-status').innerText(),/已检查 4 个已安装工具 · 0 个可升级/);
    await p.locator('.tools-search').fill('企业版');
    await p.getByRole('heading',{name:'没有匹配的工具'}).waitFor();
    await p.locator('.tools-search').fill('百度搭子');
    assert.equal(await p.locator('.tool-row').count(),1);
    await p.locator('#detect-tools').click();
    await p.getByRole('button',{name:'检查更新',exact:true}).waitFor();
    assert.equal(await p.locator('[data-tool="dumate-enterprise"]').count(),0);
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
    assert.equal(await f.page.locator('.tool-install-dialog[open]').count(),0);
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
    await detail(f.page,'codex');await f.page.locator('#tool-detail-region .tool-install-view').click();
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
    await p.evaluate(async()=>{const e=window.__fixture.entries.find(e=>e.id==='codex');e.installed=false;e.installations=[];await window.TokenMarketTools.refresh(true);});
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
    assert.deepEqual((await calls(p,'start_tool_upgrade'))[0].args,{tool:'claude-code',targetVersion:'2.1.318',planRevision:'fixture-upgrade-claude-code'});
    assert.equal(await p.locator('.tool-install-dialog[open]').count(),0);await detail(p,'claude-code');
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
test('installation details show the entry and omit resolved files and configuration paths', async () => {
  const resolvedPath='/Users/fixture/.npm-global/lib/node_modules/@openai/codex/bin/codex.js';
  const f=await openFixture({configPaths:{codex:'/Users/fixture/custom config/codex/config.toml'},installations:{codex:[{path:'/Users/fixture/.npm-global/bin/codex',resolvedPath,method:'npm',selected:true}]}});try {
    const p=f.page;
    assert.match(await p.locator('[data-tool="openclaw"] .tool-planned-method').textContent(),/npm/);
    assert.match(await p.locator('[data-tool="pi"] .tool-planned-method').textContent(),/npm/);
    await detail(p,'codex');const info=p.locator('.tool-files-sheet');
    assert.match(await info.textContent(),/安装方式npm/);assert.match(await info.textContent(),/当前入口\/Users\/fixture\/\.npm-global\/bin\/codex/);
    assert.equal(await info.getByRole('heading',{name:'安装信息',exact:true}).count(),1);
    assert.equal(await info.locator('.tool-path-row').count(),1);
    const content=await info.textContent();
    assert.doesNotMatch(content,/实际文件|配置目录|接入配置文件|尚未生成|custom config/);
    assert.equal(content.includes(resolvedPath),false);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('uninstalled applications retain the planned installation caption in list and details', async () => {
  const f=await openFixture({missingGui:true});try {
    const p=f.page;
    for (const id of ['workbuddy','deepseek-harness','dumate']) {
      assert.equal(await p.locator(`[data-tool="${id}"] .tool-planned-method`).count(),1);
      assert.equal(await p.locator(`[data-tool="${id}"] .tool-install`).isEnabled(),true);
    }
    assert.match(await p.locator('[data-tool="hermes"] .tool-planned-method').textContent(),/官方安装脚本/);
    await detail(p,'workbuddy');assert.match(await p.locator('.tool-files-sheet').textContent(),/安装方案通过桌面安装器安装/);
    assert.equal((await calls(p,'start_tool_installation')).length,0);assert.deepEqual(f.errors,[]);
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

test('signed-out startup presents a Client Update before any manual check and Later keeps login usable', async () => {
  const f=await openFixture({signedIn:false,startupClientUpdate:{status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',releaseNotes:'保留已有连接',automatic:true,remind:true,error:null}});try {
    const p=f.page;
    await p.getByRole('dialog',{name:'客户端有新版本'}).waitFor({timeout:1500});
    assert.match(await p.locator('#client-update-reminder').textContent(),/1\.1\.0/);
    assert.match(await p.locator('#client-update-reminder').textContent(),/保留已有连接/);
    assert.equal((await calls(p,'check_client_update')).length,0);
    await p.getByRole('button',{name:'稍后提醒',exact:true}).click();
    await p.waitForFunction(()=>!document.querySelector('#client-update-reminder').open);
    assert.equal(await p.locator('#login').isVisible(),true);
    assert.equal((await calls(p,'defer_client_update')).length,1);
    assert.equal((await p.evaluate(()=>window.__fixture.calls.filter(c=>/download|install|restart/.test(c.command)))).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('an automatic check failure leaves signed-out startup usable and manual retry reports its own result', async () => {
  const f=await openFixture({signedIn:false,startupClientUpdate:{status:'failed',currentVersion:'1.0.0',automatic:true,remind:false,error:'网络离线'},clientUpdate:{status:'failed',currentVersion:'1.0.0',automatic:false,remind:false,error:'清单读取失败'}});try {
    const p=f.page;
    await p.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='desktop_client_update_status'));
    assert.equal(await p.locator('#login').isVisible(),true);
    assert.equal(await p.locator('dialog[open]').count(),0);
    await p.locator('#login [data-desktop-settings]').click();
    assert.equal(await p.locator('#client-update-status').textContent(),'');
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'清单读取失败'}).waitFor();
    await p.evaluate(()=>{window.__fixture.clientUpdate={status:'current',currentVersion:'1.0.0',automatic:false,remind:false};});
    await p.getByRole('button',{name:'重试检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'没有更新的受支持稳定版本'}).waitFor();
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('a late startup event offers exact-version ignore and manual checks still show that release', async () => {
  const available={status:'available',currentVersion:'1.0.0',availableVersion:'1.1.0',releaseNotes:'稳定版说明',automatic:true,remind:true,error:null};
  const f=await openFixture({signedIn:false,deferClientUpdateStatus:true,clientUpdate:{...available,automatic:false,remind:false}});try {
    const p=f.page;
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveClientUpdateStatus));
    await p.evaluate(value=>{window.__fixture.clientUpdateStatus=value;window.__fixture.emit('desktop-client-update-status',value);},available);
    await p.getByRole('dialog',{name:'客户端有新版本'}).waitFor();
    await p.evaluate(()=>window.__fixture.resolveClientUpdateStatus());
    assert.equal(await p.locator('#client-update-reminder').isVisible(),true);
    await p.getByRole('button',{name:'忽略此版本',exact:true}).click();
    await p.waitForFunction(()=>!document.querySelector('#client-update-reminder').open);
    assert.deepEqual((await calls(p,'ignore_client_update_version')).map(c=>c.args),[{version:'1.1.0'}]);
    await p.locator('#login [data-desktop-settings]').click();
    await p.getByRole('button',{name:'检查客户端更新',exact:true}).click();
    await p.locator('#client-update-status').filter({hasText:'发现新版本 1.1.0'}).waitFor();
    assert.equal(await p.locator('dialog[open]').count(),0);
    assert.equal(await p.evaluate(()=>window.__fixture.ignoredClientUpdate),'1.1.0');
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('uninstalled filter counts missing tools and combines with search', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('[data-tool-filter="uninstalled"]').click();
    assert.equal(await p.locator('[data-tool-filter="uninstalled"]').innerText(),'未安装 11');
    assert.equal(await p.locator('.tool-row').count(),11);
    await p.locator('.tools-search').fill('OpenClaw');assert.equal(await p.locator('[data-tool="openclaw"]').count(),1);
    await p.locator('.tools-search').fill('Codex');assert.equal(await p.locator('.tool-row').count(),0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('QwenWork connection shows only usage guidance and leaves no stale list feedback', async () => {
  const f=await openFixture({installedTools:['qwenwork']});try {
    const p=f.page;const row=p.locator('[data-tool="qwenwork"]');
    await row.locator('img').evaluate(image=>image.decode());
    await row.locator('.tool-connect').click();await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.locator('.connect-confirm').click();await p.getByRole('heading',{name:'接入配置已保存'}).waitFor({timeout:2500});
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'qwenwork',recovery:null});
    const notice = await p.locator('dialog[open] .restart-notice').innerText();
    assert.match(notice,/已保存，重启千问办公后生效/);
    assert.equal(await p.getByRole('button',{name:'重启千问办公',exact:true}).isVisible(),true);
    assert.equal((await calls(p,'restart_qwenwork')).length,0);
    assert.doesNotMatch(await p.locator('dialog[open]').innerText(),/API Key|本地代理|修复配置|兼容模型|尚未测试|费用/);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    await p.locator('dialog[open] .close').click();
    assert.equal(await p.locator('#tools-region > .tools-feedback').textContent(),'');
    await detail(p,'qwenwork');
    assert.match(await p.locator('#tool-detail-region').innerText(),/接入或取消接入会保存配置，下次启动千问办公时生效/);
    assert.doesNotMatch(await p.locator('#tool-detail-region').innerText(),/API Key|本地代理|修复配置/);
    await manage(p);await p.locator('#tool-detail-region .tool-connect').filter({hasText:'重新接入'}).click();
    await p.locator('.reconnect-dialog[open]').waitFor();
    assert.doesNotMatch(await p.locator('.reconnect-dialog').innerText(),/重新生成|直接替换|尚未适配/);
    await p.locator('.reconnect-cancel').click();
    await p.locator('#tool-detail-region .tool-launch').click();
    assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'qwenwork');assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

for (const action of ['connect','reconnect','disconnect']) test(`QwenWork stopped ${action} does not prompt for restart`, async () => {
  const f=await openFixture({installedTools:['qwenwork'],qwenworkRunning:false});try {
    const p=f.page;assert.equal((await calls(p,'is_qwenwork_running')).length,0);
    await p.locator('[data-tool="qwenwork"] .tool-connect').click();await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存',exact:true}).waitFor();
    if(action!=='connect') {
      await p.locator('dialog[open] .close').click();await detail(p,'qwenwork');await manage(p);
      if(action==='reconnect') {
        await p.locator('#tool-detail-region .tool-connect').filter({hasText:'重新接入'}).click();await p.locator('.reconnect-confirm').click();
      } else {
        await p.locator('.tool-disconnect').click();await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();
      }
    }
    const title={connect:'接入配置已保存',reconnect:'重新接入配置已保存',disconnect:'取消接入配置已保存'}[action];
    const popup=p.locator('dialog[open]').filter({has:p.getByRole('heading',{name:title,exact:true})});await popup.waitFor();
    assert.equal(await popup.locator('.restart-qwenwork').isVisible(),false);
    assert.match(await popup.innerText(),/已保存，下次启动千问办公时生效/);
    assert.doesNotMatch(await popup.innerText(),/重启千问办公|确认退出/);
    if(action!=='disconnect') assert.match(await popup.innerText(),/选择 Token Market 模型/);
    assert.equal((await calls(p,'restart_qwenwork')).length,0);assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('QwenWork process detection failure preserves saved configuration and hides restart actions', async () => {
  const f=await openFixture({installedTools:['qwenwork'],qwenworkRunningError:true});try {
    const p=f.page;await p.locator('[data-tool="qwenwork"] .tool-connect').click();await p.locator('.connect-confirm').click();
    const popup=p.locator('.tool-connect-result-dialog');await popup.waitFor();
    assert.match(await popup.innerText(),/无法确认千问办公是否运行/);assert.equal(await popup.locator('.restart-qwenwork').isVisible(),false);
    assert.equal(await p.evaluate(()=>Boolean(window.__fixture.entries.find(t=>t.id==='qwenwork').connection)),true);
    await popup.locator('.close').click();await detail(p,'qwenwork');await manage(p);await p.locator('.tool-disconnect').click();
    await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();
    const result=p.locator('.tool-result-dialog');await result.waitFor();
    assert.match(await result.innerText(),/无法确认千问办公是否运行/);assert.equal(await result.locator('.restart-qwenwork').isVisible(),false);
    assert.equal(await p.evaluate(()=>window.__fixture.entries.find(t=>t.id==='qwenwork').connection),null);
    assert.equal((await calls(p,'restart_qwenwork')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('QwenWork connect reconnect and disconnect save without implicit restart', async () => {
  const f=await openFixture({installedTools:['qwenwork']});try {
    const p=f.page;await p.locator('[data-tool="qwenwork"] .tool-connect').click();
    await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存',exact:true}).waitFor();
    assert.equal((await calls(p,'restart_qwenwork')).length,0);
    await p.getByRole('button',{name:'稍后手动重启千问办公',exact:true}).click();
    await detail(p,'qwenwork');await manage(p);
    await p.locator('#tool-detail-region .tool-connect').filter({hasText:'重新接入'}).click();
    await p.locator('.reconnect-confirm').click();
    await p.getByRole('heading',{name:'重新接入配置已保存',exact:true}).waitFor();
    assert.equal((await calls(p,'restart_qwenwork')).length,0);
    await p.getByRole('button',{name:'稍后手动重启千问办公',exact:true}).click();
    await manage(p);await p.locator('.tool-disconnect').click();
    await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();
    await p.getByRole('heading',{name:'取消接入配置已保存',exact:true}).waitFor();
    assert.match(await p.locator('.tool-result-detail').innerText(),/重启千问办公后生效/);
    assert.equal(await p.locator('.tool-result-dialog .restart-qwenwork').isVisible(),true);
    assert.equal((await calls(p,'restart_qwenwork')).length,0);
    await p.getByRole('button',{name:'稍后手动重启千问办公',exact:true}).click();
    assert.equal(await p.evaluate(()=>window.__fixture.entries.find(t=>t.id==='qwenwork').connection),null);
    assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
    assert.equal((await calls(p,'is_qwenwork_running')).length,3);
  }finally{await f.close();}
});

test('QwenWork restart uses native confirmation and cancellation never undoes the saved configuration', async () => {
  const f=await openFixture({installedTools:['qwenwork']});try {
    const p=f.page;await p.locator('[data-tool="qwenwork"] .tool-connect').click();await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存',exact:true}).waitFor();
    await p.evaluate(()=>{window.__fixture.qwenworkRestartError='千问办公尚未退出，请在千问中确认退出。';});
    await p.getByRole('button',{name:'重启千问办公',exact:true}).click();
    await p.locator('.launch-error').filter({hasText:'请在千问中确认退出'}).waitFor();
    assert.equal(await p.locator('.tool-connect-result-dialog').isVisible(),true);
    assert.equal((await calls(p,'connect_tool')).length,1);
    assert.equal((await calls(p,'restart_dumate')).length,0);
    assert.equal(await p.evaluate(()=>Boolean(window.__fixture.entries.find(t=>t.id==='qwenwork').connection)),true);
    await p.evaluate(()=>{window.__fixture.qwenworkRestartError=null;});
    await p.getByRole('button',{name:'重启千问办公',exact:true}).click();
    await p.locator('.tool-connect-result-dialog').waitFor({state:'hidden'});
    assert.deepEqual((await calls(p,'restart_qwenwork')).map(call=>call.args),[{},{}]);
    assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('connection failures ask for administrator help without exposing native errors or leaving feedback', async () => {
  for (const command of ['prepare_tool_connect','prepare_tool_reconnect','connect_tool','reconnect_tool']) {
    const reconnect=command.includes('reconnect');
    const f=await openFixture({multi:reconnect});try {
      const p=f.page;
      await p.evaluate(command=>{
        const invoke=window.__TAURI__.core.invoke;
        window.__TAURI__.core.invoke=(name,args)=>name===command ? Promise.reject(Error('内部实现：凭证校验 SQL naming_version 失败')) : invoke(name,args);
      },command);
      if(reconnect) {
        await detail(p,'workbuddy');await manage(p);
        await p.locator('#tool-detail-region .tool-connect').filter({hasText:'重新接入'}).click();
        if(command==='reconnect_tool')await p.locator('.reconnect-confirm').click();
      } else {
        await p.locator('[data-tool="workbuddy"] .tool-connect').click();
        if(command==='connect_tool')await p.locator('.connect-confirm').click();
      }
      const result=p.locator('.tool-result-dialog[open]');await result.waitFor();
      assert.equal(await result.locator('h3').textContent(),'接入失败');
      assert.equal(await result.locator('.tool-result-detail').textContent(),'请联系管理员。');
      assert.doesNotMatch(await p.locator('body').innerText(),/内部实现|naming_version/);
      await result.locator('.result-close').click();
      assert.equal(await p.locator('#tools-region > .tools-feedback').textContent(),'');
      assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});

test('tool action menus dismiss outside, on Escape and navigation without leaving the detail page', async () => {
  const f=await openFixture();try {
    const p=f.page;
    const listMenu=p.locator('[data-tool="codex"] .tool-lifecycle-menu');
    await listMenu.locator('summary').click();assert.equal(await listMenu.getAttribute('open'),'');
    await p.locator('.tools-search').click();assert.equal(await listMenu.getAttribute('open'),null);
    await listMenu.locator('summary').click();await p.keyboard.press('Escape');
    assert.equal(await listMenu.getAttribute('open'),null);
    assert.equal(await listMenu.locator('summary').evaluate(node=>node===document.activeElement),true);
    await detail(p,'codex');
    const more=p.locator('#tool-detail-region .tool-lifecycle-menu');
    const connection=p.locator('#tool-detail-region .tool-menu');
    await more.locator('summary').click();await connection.locator('summary').click();
    assert.equal(await more.getAttribute('open'),null);assert.equal(await connection.getAttribute('open'),'');
    await p.keyboard.press('Escape');assert.equal(await connection.getAttribute('open'),null);
    assert.equal(await p.locator('#tool-detail-page').isVisible(),true);
    assert.equal(await connection.locator('summary').evaluate(node=>node===document.activeElement),true);
    await more.locator('summary').focus();await p.keyboard.press('Enter');await p.keyboard.press('Shift+Tab');
    assert.equal(await more.getAttribute('open'),null);
    await more.locator('summary').click();await p.locator('#tool-detail-region h1').click();
    assert.equal(await more.getAttribute('open'),null);
    await more.locator('summary').click();await p.evaluate(()=>window.TokenMarketShell.show('account'));
    assert.equal(await more.getAttribute('open'),null);
    await p.evaluate(()=>window.TokenMarketShell.show('detail','Codex'));
    await more.locator('summary').click();await more.locator('.tool-uninstall').click();
    await p.locator('.tool-uninstall-confirm[open]').waitFor();assert.equal(await more.getAttribute('open'),null);
    assert.equal((await calls(p,'start_tool_uninstall')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('connection menu actions stay clickable after a detail refresh', async () => {
  const f=await openFixture({colorScheme:'dark'});try {
    const p=f.page;await p.locator('.tools-updates-status').filter({hasText:'已检查'}).waitFor();
    await detail(p,'codex');
    const menu=p.locator('#tool-detail-region .tool-menu');
    await menu.locator('summary').click();
    await p.evaluate(async()=>{window.__fixture.entries.find(e=>e.id==='codex').connection.models=['refreshed-model'];await window.TokenMarketTools.refresh(true);});
    assert.equal(await menu.getAttribute('open'),'');
    for(const [action,dialog] of [['.tool-connect','.reconnect-dialog'],['.tool-disconnect','.disconnect-confirm-dialog']]) {
      if(await menu.getAttribute('open')===null)await menu.locator('summary').click();
      const button=menu.locator(action);
      assert.equal(await button.isEnabled(),true);
      await button.click({timeout:2500});
      assert.equal(await p.locator(dialog).isVisible(),true);
      await p.keyboard.press('Escape');
    }
    for(const command of ['reconnect_tool','disconnect_tool'])assert.equal((await calls(p,command)).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('ccswitch display hides unknown sources and keeps broken installs available for uninstall', async () => {
  const f=await openFixture({brokenTool:'claude-code'});try {
    const p=f.page;await p.waitForFunction(()=>window.__fixture.calls.some(c=>c.command==='check_tool_updates'));
    const row=p.locator('[data-tool="claude-code"]');await row.getByText('无法运行',{exact:true}).waitFor();
    assert.equal(await row.locator('.tool-install-method').count(),0);assert.equal(await row.locator('.tool-install,.tool-upgrade').count(),0);
    await row.locator('.tool-lifecycle-menu summary').click();await row.locator('.tool-uninstall').click();
    await p.locator('.tool-uninstall-confirm[open]').waitFor();assert.equal((await calls(p,'start_tool_uninstall')).length,0);
    await p.locator('.tool-uninstall-confirm').getByRole('button',{name:'取消',exact:true}).click();assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('uninstall previews and removes only the selected install while retaining the connection', async () => {
  const sources=[{path:'/Users/fixture/.local/bin/codex',method:'原生程序',selected:true},{path:'/Users/fixture/.npm-global/bin/codex',method:'npm',selected:false}];
  const f=await openFixture({installations:{codex:sources}});try {
    const p=f.page;await p.locator('[data-tool="codex"] .tool-lifecycle-menu summary').click();await p.locator('[data-tool="codex"] .tool-uninstall').click();
    const dialog=p.locator('.tool-uninstall-confirm');await dialog.waitFor();assert.equal(await dialog.locator('.tool-uninstall-execute').isDisabled(),true);
    await dialog.locator('input[type="radio"]').nth(1).check();assert.match(await dialog.locator('pre').textContent(),/npm-global/);
    assert.equal((await calls(p,'start_tool_uninstall')).length,0);await dialog.locator('.tool-uninstall-execute').click();
    await p.waitForFunction(()=>window.__fixture.installation?.stage==='succeeded');
    const cmd=(await calls(p,'start_tool_uninstall'))[0];assert.equal(cmd.args.plan.path,sources[1].path);
    const entry=await p.evaluate(()=>window.__fixture.entries.find(e=>e.id==='codex'));assert.equal(entry.installed,true);assert.equal(entry.installations.length,1);assert.ok(entry.connection);
    for(const command of ['connect_tool','disconnect_tool','test_tool_connection','start_tool_installation'])assert.equal((await calls(p,command)).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('unknown uninstall channels display the manual reason without enabling execution', async () => {
  const f=await openFixture({manualUninstall:'codex'});try {
    const p=f.page;await p.locator('[data-tool="codex"] .tool-lifecycle-menu summary').click();await p.locator('[data-tool="codex"] .tool-uninstall').click();
    const dialog=p.locator('.tool-uninstall-confirm');await dialog.waitFor();assert.equal(await dialog.locator('.tool-uninstall-execute').isDisabled(),true);assert.match(await dialog.textContent(),/原安装器/);
    assert.equal((await calls(p,'start_tool_uninstall')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});


test('DuMate chooses a model on connect and switches it in details without changing billing', async () => {
  const f=await openFixture({installedTools:['dumate']});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();
    assert.equal(await p.locator('.dumate-model-select option').count(),2);
    assert.equal((await calls(p,'configure_dumate_model')).length,0);
    await p.locator('.dumate-model-select').selectOption('other-model');await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    assert.equal((await calls(p,'configure_dumate_model'))[0].args.model,'other-model');
    await p.locator('dialog[open] .close').click();
    assert.match(await p.locator('.tool-model-entry').innerText(),/other-model/);
    await p.locator('.primary-nav [data-page="account"]').click();await p.locator('.account-switch-action').click();
    await p.locator('.workspace-switch-dialog [data-workspace-id="team"]').click();
    await p.locator('.primary-nav [data-page="tools"]').click();await detail(p,'dumate');await p.locator('.tool-switch-model').click();
    await p.locator('.dumate-model-select:enabled').waitFor();
    assert.equal(await p.locator('.dumate-model-select').inputValue(),'other-model');
    assert.match(await p.locator('.dumate-model-note').innerText(),/保留当前密钥和计费空间/);
    assert.doesNotMatch(await p.locator('.dumate-model-note').innerText(),/请先完全退出/);
    assert.match(await p.locator('.dumate-model-target').innerText(),/个人空间/);
    await p.locator('.dumate-model-select').selectOption('fixture-model');await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'模型切换配置已保存'}).waitFor();
    assert.match(await p.locator('.restart-notice').innerText(),/已保存，重启搭子后生效/);
    assert.doesNotMatch(await p.locator('.restart-notice').innerText(),/普通聊天可能仍使用搭子原生模型/);
    assert.doesNotMatch(await p.locator('.restart-notice').innerText(),/配置已生效/);
    assert.equal((await calls(p,'restart_dumate')).length,0);
    assert.equal(await p.locator('dialog[open] .restart-dumate').isVisible(),true);
    const changed=(await calls(p,'configure_dumate_model')).at(-1).args;
    assert.equal(changed.target.preserveBinding,true);assert.equal(changed.model,'fixture-model');
    assert.equal(await p.evaluate(()=>window.__fixture.entries.find(t=>t.id==='dumate').connection.boundWorkspace.id),'personal');
    assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('DuMate model loading failure, cancellation and workspace changes cannot save a model', async () => {
  const f=await openFixture({installedTools:['dumate'],dumateModelError:true});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-status').filter({hasText:'无法获取模型目录'}).waitFor();
    assert.equal(await p.locator('.dumate-model-confirm').isDisabled(),true);
    await p.evaluate(()=>{window.__fixture.dumateModelError=false;});
    await p.locator('.dumate-model-retry').click();await p.locator('.dumate-model-select:enabled').waitFor();
    await p.evaluate(async()=>{window.__fixture.current=window.__fixture.team;await window.TokenMarketWorkspace.refresh();});
    assert.equal(await p.locator('.dumate-model-dialog').isVisible(),false);assert.equal((await calls(p,'configure_dumate_model')).length,0);
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
  const delayed=await openFixture({installedTools:['dumate']});try {
    const p=delayed.page;await p.evaluate(()=>window.__fixture.deferDuMateModels=true);
    await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveDuMateModels));await p.locator('.dumate-model-cancel').click();
    await p.evaluate(()=>{window.__fixture.resolveDuMateModels();});
    assert.equal(await p.locator('.dumate-model-dialog').isVisible(),false);
    assert.equal((await calls(p,'configure_dumate_model')).length,0);assert.deepEqual(delayed.errors,[]);
  }finally{await delayed.close();}
});


test('DuMate recovers a missing credential before choosing and saving a model', async () => {
  const f=await openFixture({installedTools:['dumate'],dumateCredentialLost:true});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.recovery-dialog[open]').waitFor();assert.equal((await calls(p,'configure_dumate_model')).length,0);
    await p.locator('.recovery-confirm').click();await p.locator('.dumate-model-select:enabled').waitFor();
    await p.locator('.dumate-model-select').selectOption('other-model');await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    const request=(await calls(p,'configure_dumate_model'))[0].args;
    assert.equal(request.model,'other-model');assert.equal(request.target.recovery.keyId,'missing-dumate');assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});


test('DuMate sends the workspace owner ID when the display profile has no ID', async () => {
  const f=await openFixture({installedTools:['dumate']});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.waitForFunction(()=>window.__fixture.calls.some(call=>call.command==='prepare_dumate_models'));
    const request=(await calls(p,'prepare_dumate_models'))[0].args;
    assert.equal(request.target.expectedOwnerId,'user-fixture',await p.locator('.dumate-model-status').innerText());
    await p.locator('.dumate-model-select:enabled').waitFor();
    await p.locator('.dumate-model-confirm').click();await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    assert.equal((await calls(p,'configure_dumate_model'))[0].args.target.expectedOwnerId,'user-fixture');
    assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});




for (const action of ['connect','reconnect','switch','disconnect']) test(`DuMate stopped ${action} does not prompt for restart`, async () => {
  const f=await openFixture({installedTools:['dumate'],dumateRunning:false});try {
    const p=f.page;await detail(p,'dumate');assert.equal((await calls(p,'is_dumate_running')).length,0);
    await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存',exact:true}).waitFor();
    if (action!=='connect') {
      await p.locator('dialog[open] .close').click();
      if (action==='switch') await p.locator('.tool-switch-model').click();
      else {
        await manage(p);
        if (action==='disconnect') await p.locator('.tool-disconnect').click();
        else await p.locator('#tool-detail-region .tool-menu[open]').getByRole('button',{name:'重新接入',exact:true}).click();
      }
      if (action==='disconnect') await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();
      else {
        await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
      }
    }
    const title={connect:'接入配置已保存',reconnect:'重新接入配置已保存',switch:'模型切换配置已保存',disconnect:'取消接入配置已保存'}[action];
    const popup=p.locator('dialog[open]').filter({has:p.getByRole('heading',{name:title,exact:true})});await popup.waitFor();
    assert.equal(await popup.locator('.restart-dumate').isVisible(),false);
    assert.match(await popup.innerText(),/下次启动搭子时生效/);
    assert.doesNotMatch(await popup.innerText(),/重启搭子/);
    assert.equal((await calls(p,'restart_dumate')).length,0);
    assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('DuMate save reconnect switch and cancellation only save configuration until the user chooses restart', async () => {
  const f=await openFixture({installedTools:['dumate']});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    assert.match(await p.locator('.restart-notice').innerText(),/已保存，重启搭子后生效/);
    assert.equal(await p.getByRole('button',{name:'重启搭子',exact:true}).isVisible(),true);
    assert.equal(await p.getByRole('button',{name:'稍后手动重启搭子',exact:true}).isVisible(),true);
    assert.equal((await calls(p,'restart_dumate')).length,0);
    await p.locator('dialog[open] .close').click();await p.locator('.tool-switch-model').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-select').selectOption('other-model');await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'模型切换配置已保存'}).waitFor();
    assert.match(await p.locator('.restart-notice').innerText(),/other-model.*已保存，重启搭子后生效/);
    assert.equal((await calls(p,'restart_dumate')).length,0);
    await p.locator('dialog[open] .close').click();await manage(p);await p.locator('#tool-detail-region .tool-menu[open]').getByRole('button',{name:'重新接入',exact:true}).click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'重新接入配置已保存'}).waitFor();
    assert.equal((await calls(p,'restart_dumate')).length,0);
    await p.locator('dialog[open] .close').click();await manage(p);await p.locator('.tool-disconnect').click();
    await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();
    await p.getByRole('heading',{name:'取消接入配置已保存'}).waitFor();
    assert.match(await p.locator('.tool-result-detail').innerText(),/已保存，重启搭子后生效/);
    assert.equal(await p.locator('.tool-result-dialog[open] .restart-dumate').isVisible(),true);
    assert.equal((await calls(p,'restart_dumate')).length,0);
    assert.equal((await calls(p,'is_dumate_running')).length,4);assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('DuMate process detection failure keeps the saved configuration and uses conditional guidance', async () => {
  const f=await openFixture({installedTools:['dumate'],dumateRunningError:true});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-select').selectOption('other-model');await p.locator('.dumate-model-confirm').click();
    const popup=p.locator('.tool-connect-result-dialog');await popup.waitFor();
    assert.match(await popup.innerText(),/无法确认搭子是否运行/);
    assert.equal(await popup.locator('.restart-dumate').isVisible(),false);
    assert.deepEqual(await p.evaluate(()=>window.__fixture.entries.find(entry=>entry.id==='dumate').connection.models),['other-model']);
    assert.equal((await calls(p,'restart_dumate')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('DuMate restart always warns about running and scheduled tasks before confirmation', async () => {
  const f=await openFixture({installedTools:['dumate']});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    await p.getByRole('button',{name:'重启搭子',exact:true}).click();
    const confirmation=p.locator('.dumate-restart-confirm');await confirmation.waitFor({timeout:1000});
    assert.match(await confirmation.innerText(),/所有正在运行的任务都将被中断，定时任务也将无法调度。/);
    assert.equal((await calls(p,'restart_dumate')).some(call=>call.args.force===true),false);
    await confirmation.getByRole('button',{name:'取消',exact:true}).click();
    assert.equal(await p.locator('.tool-connect-result-dialog').isVisible(),true);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('DuMate process identity errors leave the saved model unchanged', async () => {
  for(const error of ['无法确认搭子进程身份，未执行重启。请手动重启。']) {
    const f=await openFixture({installedTools:['dumate'],dumateRestartError:error});try {
      const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
      await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-select').selectOption('other-model');await p.locator('.dumate-model-confirm').click();
      await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();assert.equal((await calls(p,'restart_dumate')).length,0);
      const snapshot=await p.evaluate(()=>JSON.stringify(window.__fixture.entries.find(entry=>entry.id==='dumate').connection));
      await p.locator('dialog[open] .restart-dumate').click();await p.locator('.dumate-restart-confirm .force-restart-dumate').click();await p.locator('.launch-error').filter({hasText:'未执行重启'}).waitFor();
      assert.equal(await p.evaluate(()=>JSON.stringify(window.__fixture.entries.find(entry=>entry.id==='dumate').connection)),snapshot);
      assert.equal((await calls(p,'configure_dumate_model')).length,1);assert.equal((await calls(p,'restart_dumate')).length,2);
      await p.evaluate(()=>{window.__fixture.dumateRestartError=null;});
      await p.locator('dialog[open] .restart-dumate').click();await p.locator('.dumate-restart-confirm .force-restart-dumate').click();await p.locator('.tool-connect-result-dialog').waitFor({state:'hidden'});
      assert.equal((await calls(p,'restart_dumate')).length,4);assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});

test('DuMate restart remains available after cancellation removes its connection', async () => {
  const f=await openFixture({installedTools:['dumate']});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    await p.locator('dialog[open] .close').click();await manage(p);await p.locator('.tool-disconnect').click();
    await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();await p.getByRole('heading',{name:'取消接入配置已保存'}).waitFor();
    assert.equal(await p.evaluate(()=>window.__fixture.entries.find(entry=>entry.id==='dumate').connection),null);
    await p.locator('.tool-result-dialog[open] .restart-dumate').click();await p.locator('.dumate-restart-confirm .force-restart-dumate').click();await p.locator('.tool-result-dialog').waitFor({state:'hidden'});
    assert.equal((await calls(p,'restart_dumate')).length,2);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});


test('DuMate restart always asks once and requires explicit force after connect or cancellation', async () => {
  for (const disconnect of [false,true]) {
    const f=await openFixture({installedTools:['dumate'],dumateRestartBusy:true});try {
      const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
      await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
      await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
      if(disconnect) {
        await p.getByRole('button',{name:'稍后手动重启搭子',exact:true}).click();
        await manage(p);await p.locator('.tool-disconnect').click();
        await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();
        await p.getByRole('heading',{name:'取消接入配置已保存'}).waitFor();
      }
      const snapshot=await p.evaluate(()=>JSON.stringify(window.__fixture.entries.find(entry=>entry.id==='dumate').connection));
      const confirmation=p.locator('.dumate-restart-confirm');
      await p.getByRole('button',{name:'重启搭子',exact:true}).click();await confirmation.waitFor();
      assert.match(await confirmation.innerText(),/所有正在运行的任务都将被中断，定时任务也将无法调度。/);
      assert.deepEqual((await calls(p,'restart_dumate')).map(call=>call.args.force),[false]);
      assert.equal(await p.evaluate(()=>document.activeElement.textContent),'取消');
      await confirmation.getByRole('button',{name:'取消',exact:true}).click();await confirmation.waitFor({state:'hidden'});
      assert.equal((await calls(p,'restart_dumate')).length,1);
      assert.equal(await p.evaluate(()=>JSON.stringify(window.__fixture.entries.find(entry=>entry.id==='dumate').connection)),snapshot);
      await p.getByRole('button',{name:'重启搭子',exact:true}).click();await confirmation.waitFor();
      await confirmation.getByRole('button',{name:'强制重启搭子',exact:true}).click();
      await p.locator(disconnect?'.tool-result-dialog':'.tool-connect-result-dialog').waitFor({state:'hidden'});
      assert.deepEqual((await calls(p,'restart_dumate')).map(call=>call.args.force),[false,false,true]);
      assert.equal(await confirmation.isVisible(),false);
      assert.equal(await p.evaluate(()=>JSON.stringify(window.__fixture.entries.find(entry=>entry.id==='dumate').connection)),snapshot);
      assert.equal((await calls(p,'configure_dumate_model')).length,1);assert.equal((await calls(p,'test_tool_connection')).length,0);
      assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});

test('DuMate restart confirmation cancels on Escape or logout without a force request', async () => {
  const f=await openFixture({installedTools:['dumate'],dumateRestartBusy:true});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    await p.getByRole('button',{name:'重启搭子',exact:true}).click();await p.locator('.dumate-restart-confirm').waitFor();
    await p.keyboard.press('Escape');await p.locator('.dumate-restart-confirm').waitFor({state:'hidden'});
    await p.getByRole('button',{name:'重启搭子',exact:true}).click();await p.locator('.dumate-restart-confirm').waitFor();
    await p.evaluate(()=>window.TokenMarketTools.setAuthenticated(false));
    await p.locator('.dumate-restart-confirm').waitFor({state:'hidden'});
    assert.deepEqual((await calls(p,'restart_dumate')).map(call=>call.args.force),[false,false]);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});


test('DuMate closing the saved popup during restart preparation cannot open a late force confirmation', async () => {
  const f=await openFixture({installedTools:['dumate'],dumateRestartBusy:true});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
    await p.getByRole('heading',{name:'接入配置已保存'}).waitFor();
    await p.evaluate(()=>{
      const original=window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke=(command,args)=>command==='restart_dumate'?new Promise(resolve=>{window.__fixture.releaseRestart=()=>resolve(original(command,args));}):original(command,args);
    });
    await p.getByRole('button',{name:'重启搭子',exact:true}).click();
    await p.waitForFunction(()=>Boolean(window.__fixture.releaseRestart));
    await p.getByRole('button',{name:'稍后手动重启搭子',exact:true}).click();
    await p.evaluate(()=>window.__fixture.releaseRestart());
    await p.getByRole('button',{name:'重启搭子',exact:true}).waitFor({state:'hidden'});
    assert.equal(await p.locator('.dumate-restart-confirm').isVisible(),false);
    assert.deepEqual((await calls(p,'restart_dumate')).map(call=>call.args.force),[false]);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('DuMate restart success closes the saved popup without an extra footer message', async () => {
  for(const action of ['connect','switch','disconnect']) {
    const f=await openFixture({installedTools:['dumate']});try {
      const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
      await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
      await p.getByRole('heading',{name:'接入配置已保存',exact:true}).waitFor();
      let title='接入配置已保存';
      if(action!=='connect') {
        await p.getByRole('button',{name:'稍后手动重启搭子',exact:true}).click();
        if(action==='switch') {
          await p.locator('.tool-switch-model').click();await p.locator('.dumate-model-select:enabled').waitFor();
          await p.locator('.dumate-model-select').selectOption('other-model');await p.locator('.dumate-model-confirm').click();title='模型切换配置已保存';
        } else {
          await manage(p);await p.locator('.tool-disconnect').click();
          await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入',exact:true}).click();title='取消接入配置已保存';
        }
      }
      const popup=p.locator('dialog').filter({has:p.getByRole('heading',{name:title,exact:true,includeHidden:true})});await popup.waitFor();
      await popup.getByRole('button',{name:'重启搭子',exact:true}).click();
      await p.locator('.dumate-restart-confirm .force-restart-dumate').click();
      await popup.waitFor({state:'hidden',timeout:1500});
      assert.equal(await popup.locator('.tool-actions ~ p').count(),0);
      assert.equal(await popup.locator('.launch-error,.dumate-restart-status').textContent(),'');
      assert.equal((await calls(p,'restart_dumate')).length,2);assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});


test('DuMate saved popup can be closed after a restart error or with Escape during a pending check', async () => {
  const f=await openFixture({installedTools:['dumate'],dumateRestartError:'无法确认搭子进程身份，请手动重启搭子。'});try {
    const p=f.page;await detail(p,'dumate');await p.locator('#tool-detail-region .tool-connect').click();
    await p.locator('.dumate-model-select:enabled').waitFor();await p.locator('.dumate-model-confirm').click();
    const popup=p.locator('.tool-connect-result-dialog');await popup.waitFor();
    await popup.getByRole('button',{name:'重启搭子',exact:true}).click();
    await p.locator('.dumate-restart-confirm .force-restart-dumate').click();
    await popup.locator('.launch-error').filter({hasText:'无法确认'}).waitFor();
    assert.equal(await popup.locator('.tool-actions ~ p').count(),0);
    const error=await popup.locator('.launch-error').boundingBox(),actions=await popup.locator('.tool-actions').boundingBox();
    assert.ok(error.y+error.height<=actions.y);
    await popup.getByRole('button',{name:'稍后手动重启搭子',exact:true}).click();await popup.waitFor({state:'hidden'});
    await p.locator('.tool-switch-model').click();await p.locator('.dumate-model-select:enabled').waitFor();
    await p.locator('.dumate-model-confirm').click();await popup.waitFor();
    await p.evaluate(()=>{
      const original=window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke=(command,args)=>command==='restart_dumate'?new Promise(resolve=>{window.__fixture.releaseRestart=()=>resolve({state:'restarted'});}):original(command,args);
    });
    await popup.getByRole('button',{name:'重启搭子',exact:true}).click();await p.keyboard.press('Escape');await popup.waitFor({state:'hidden'});
    await p.evaluate(()=>window.__fixture.releaseRestart());
    assert.equal(await p.locator('dialog[open]').count(),0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
