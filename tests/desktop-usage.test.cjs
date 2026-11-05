const {test,before,after}=require('node:test');
const assert=require('node:assert/strict');
const {chromium}=require('playwright');
const fixture=require('./helpers/desktop-fixture.cjs');
let browser;
before(async()=>{browser=await chromium.launch({headless:true,...(process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL?{channel:process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL}:{})});});
after(async()=>{await browser?.close();});
const calls=(p,command)=>p.evaluate(command=>window.__fixture.calls.filter(c=>c.command===command),command);
async function open(options={}){const f=await fixture(browser,options);await f.page.locator('[data-page="usage"]').click();return f;}

test('summary covers all historical keys while detail pages do not refetch buckets',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 assert.equal(await p.locator('#usage-requests').textContent(),'21');assert.equal(await p.locator('#usage-rows tr').count(),20);
 assert.equal(await p.locator('#usage-cost').textContent(),'2.100000');assert.equal(await p.locator('#usage-cache').textContent(),'80.0%');
 assert.match(await p.locator('#usage-key-count').textContent(),/2 个历史 Key/);assert.match(await p.locator('#usage-rows').textContent(),/Codex 历史 Key/);assert.match(await p.locator('#usage-rows').textContent(),/Codex 新 Key/);
 const buckets=(await calls(p,'desktop_usage_buckets')).length;await p.locator('#usage-model').fill('unsubmitted');await p.locator('#usage-next').click();await p.locator('#usage-rows [data-request-id="usage-20"]').waitFor();
 assert.equal(await p.locator('#usage-rows tr').count(),1);assert.equal(await p.locator('#usage-requests').textContent(),'21');assert.equal((await calls(p,'desktop_usage_buckets')).length,buckets);
 assert.equal((await calls(p,'desktop_usage_logs')).at(-1).args.request.model,'');assert.equal(await p.locator('#usage-next').isDisabled(),true);
 await p.locator('#usage-prev').click();await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();assert.equal(await p.locator('#usage-rows tr').count(),20);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('model and status filters refresh both datasets and render empty states',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 await p.locator('#usage-model').fill('missing-model');await p.locator('#usage-filters button').click();await p.getByText('没有符合条件的请求日志。').waitFor();assert.equal(await p.locator('#usage-requests').textContent(),'0');
 await p.locator('#usage-model').fill('fixture-model');await p.locator('#usage-filters button').click();await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 await p.locator('#usage-status').selectOption('FAILED');await p.getByText('没有符合条件的请求日志。').waitFor();
 for(const command of ['desktop_usage_logs','desktop_usage_buckets'])assert.equal((await calls(p,command)).at(-1).args.request.status,'FAILED');assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('scope changes discard old requests and clear private rows on logout',async()=>{
 const f=await open({deferUsage:true});try{const p=f.page;await p.waitForFunction(()=>window.__fixture.usageDeferred.length===2);
 await p.evaluate(()=>{window.__fixture.current=window.__fixture.team;window.dispatchEvent(new CustomEvent('current-workspace-changed',{detail:window.__fixture.team}));});
 await p.waitForFunction(()=>window.__fixture.usageDeferred.length===4);
 await p.evaluate(()=>{const pending=window.__fixture.usageDeferred.splice(0);window.__fixture.oldUsage=pending.slice(0,2);pending.slice(2).forEach(resolve=>resolve());});
 await p.locator('#usage-rows [data-request-id="team-only"]').waitFor();
 // Resolve old personal requests last; they must not replace team data.
 await p.evaluate(async()=>{window.__fixture.oldUsage.forEach(resolve=>resolve());await new Promise(requestAnimationFrame);});
 assert.equal(await p.locator('#usage-rows [data-request-id="team-only"]').count(),1);assert.equal(await p.locator('#usage-rows [data-request-id="usage-0"]').count(),0);
 await p.evaluate(()=>{window.__fixture.emit('desktop-login-status',{context:null});});
 assert.equal(await p.locator('#usage-rows [data-request-id]').count(),0);assert.equal(await p.locator('#usage-cost').textContent(),'—');assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('usage is independent of current connections, escapes model text and supports dark mode',async()=>{
 const f=await fixture(browser,{colorScheme:'dark',usageModel:'<img src=x onerror=alert(1)>'});try{const p=f.page;
 await p.evaluate(()=>{window.__fixture.entries.forEach(entry=>entry.connection=null);});await p.locator('[data-page="usage"]').click();await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 assert.equal(await p.locator('#usage-requests').textContent(),'21');assert.equal(await p.locator('#usage-rows img').count(),0);assert.match(await p.locator('#usage-rows').textContent(),/<img src=x/);
 assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('summary failure still allows paginated detail and retry is available',async()=>{
 const f=await open({usageSummaryError:true});try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();await p.locator('#usage-feedback').waitFor();assert.equal(await p.locator('#usage-cost').textContent(),'—');assert.equal(await p.locator('#usage-next').isEnabled(),true);assert.equal(await p.locator('#usage-refresh').isEnabled(),true);assert.deepEqual(f.errors,[]);}finally{await f.close();}
});
