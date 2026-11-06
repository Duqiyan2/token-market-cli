const {test} = require('node:test');
const assert = require('node:assert/strict');
const {chromium} = require('playwright');
const openFixture = require('./helpers/desktop-fixture.cjs');

test('native downloads stay inline, allow other actions, and reset after opening the installer', async () => {
  const browser = await chromium.launch({headless:true});
  const f = await openFixture(browser, {missingGui:true,installPending:true});
  try {
    const p = f.page, row = p.locator('[data-tool="workbuddy"]');
    await row.locator('.tool-install').click();
    await p.waitForFunction(() => Boolean(window.__fixture.installation));
    const update = state => p.evaluate(state => {window.__fixture.installation = {...window.__fixture.installation,...state};}, state);
    await update({stage:'downloading',detail:'正在下载安装包',downloadProgress:{downloadedBytes:26214400,totalBytes:104857600}});
    await row.locator('[aria-busy="true"]').waitFor();
    await row.locator('.tool-download-inline').filter({hasText:'25%'}).waitFor();
    assert.equal(await p.locator('dialog[open]').count(),0);
    assert.equal(await row.locator('.tool-install').isDisabled(),true);
    assert.equal(await p.locator('[data-tool="opencode"] .tool-install').isEnabled(),true);
    assert.equal(await p.locator('[data-tool="codex"] .tool-launch').isEnabled(),true);
    await p.locator('[data-page="usage"]').first().click();
    await p.locator('[data-page="tools"]').first().click();
    await row.locator('.tool-detail').click();
    await p.locator('#tool-detail-region .tool-install-view').click();
    const bar = p.locator('.tool-install-dialog progress');
    assert.equal(await bar.getAttribute('value'),'25');
    await update({stage:'downloading',downloadProgress:{downloadedBytes:52428800,totalBytes:null}});
    await p.waitForFunction(() => !document.querySelector('.tool-install-dialog progress').hasAttribute('value'));
    assert.match(await p.locator('.tool-install-dialog .tool-download-summary').textContent(), /50.*MiB.*总大小未知/);
    assert.equal(await p.getByRole('button',{name:'取消下载',exact:true}).isVisible(),true);
    await p.keyboard.press('Escape');
    assert.equal(await p.locator('dialog[open]').count(),0);
    assert.equal((await p.evaluate(() => window.__fixture.calls.filter(c=>c.command==='cancel_tool_installation'))).length,0);
    await p.locator('[data-page="tools"]').first().click();
    await update({stage:'installer_opened',detail:'安装程序已打开',downloadProgress:null});
    await row.locator('.tool-install:enabled').waitFor();
    assert.equal(await row.locator('button[aria-busy="true"]').count(),0);
    assert.equal(await row.locator('.tool-native-install-pending[aria-busy="true"]').textContent(),'未安装');
    await row.locator('.tool-install').click();
    await p.waitForFunction(() => window.__fixture.calls.filter(c=>c.command==='start_tool_installation').length===2);
    await update({stage:'installer_opened',detail:'安装程序已打开'});
    await row.locator('.tool-install:enabled').waitFor();
    assert.equal(await row.locator('.tool-native-install-pending').isVisible(),true);
    await p.evaluate(() => {
      const entry = window.__fixture.entries.find(e=>e.id==='workbuddy');entry.installed=true;entry.canConnect=true;
      window.__fixture.updateVersions.workbuddy='5.9.5';
      window.dispatchEvent(new Event('focus'));
    });
    await row.locator('.tool-connect:enabled').waitFor();
    await row.locator('.tool-version').filter({hasText:'5.9.5'}).waitFor();
    assert.equal(await row.locator('.tool-native-install-pending').count(),0);
    assert.deepEqual(f.errors,[]);
  } finally {await f.close();await browser.close();}
});

test('CLI installs refresh actual tool status without opening a progress dialog', async () => {
  const browser = await chromium.launch({headless:true});
  const f = await openFixture(browser);
  try {
    const p=f.page,row=p.locator('[data-tool="opencode"]');
    await row.locator('.tool-install').click();
    await row.locator('.tool-connect:enabled').waitFor();
    assert.equal(await p.locator('dialog[open]').count(),0);
    assert.equal((await p.evaluate(()=>window.__fixture.calls.filter(c=>c.command==='connect_tool'))).length,0);
    assert.deepEqual(f.errors,[]);
  } finally {await f.close();await browser.close();}
});

test('two tools can install independently without replacing each other’s task', async () => {
  const browser = await chromium.launch({headless:true});
  const f = await openFixture(browser);
  try {
    const p=f.page;
    await p.evaluate(() => {
      const original=window.__TAURI__.core.invoke;
      window.__fixture.jobs={};
      window.__TAURI__.core.invoke=async (command,args={}) => {
        const fixture=window.__fixture;
        if(command==='start_tool_installation') {
          fixture.calls.push({command,args});
          return fixture.jobs[args.tool]={tool:args.tool,stage:'downloading',detail:'正在下载',downloadProgress:{downloadedBytes:10,totalBytes:100},output:[]};
        }
        if(command==='tool_installation_status') return structuredClone(fixture.jobs[args.tool]);
        return original(command,args);
      };
    });
    for (const tool of ['opencode','hermes']) await p.locator(`[data-tool="${tool}"] .tool-install`).click();
    for (const tool of ['opencode','hermes']) assert.equal(await p.locator(`[data-tool="${tool}"] [aria-busy="true"]`).isVisible(),true);
    assert.equal(await p.locator('dialog[open]').count(),0);
    await p.evaluate(() => {
      for(const [tool,state] of Object.entries(window.__fixture.jobs)) {
        state.stage='succeeded';state.detail='安装完成';
        const entry=window.__fixture.entries.find(e=>e.id===tool);entry.installed=true;entry.canConnect=true;
      }
    });
    for (const tool of ['opencode','hermes']) await p.locator(`[data-tool="${tool}"] .tool-connect:enabled`).waitFor();
    assert.deepEqual(f.errors,[]);
  } finally {await f.close();await browser.close();}
});

test('client updates display byte progress and hide it during signature verification', async () => {
  const browser = await chromium.launch({headless:true});
  const f=await openFixture(browser,{signedIn:false});
  try {
    const p=f.page;await p.locator('#login [data-desktop-settings]').click();
    const send = value => p.evaluate(value => window.__fixture.emit('desktop-client-update-status',{currentVersion:'1.0.0',availableVersion:'1.1.0',...value}),value);
    await send({status:'downloading',downloadProgress:{downloadedBytes:26214400,totalBytes:104857600}});
    const progress=p.locator('.client-update-result progress');
    assert.equal(await progress.isVisible(),true);
    assert.equal(await progress.getAttribute('value'),'25');
    await send({status:'verifying'});
    assert.equal(await progress.isVisible(),false);
    assert.match(await p.locator('#client-update-status').textContent(),/验证更新包签名/);
    assert.deepEqual(f.errors,[]);
  } finally {await f.close();await browser.close();}
});
