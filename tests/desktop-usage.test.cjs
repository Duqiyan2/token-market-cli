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
 assert.equal(await p.locator('#usage-requests').textContent(),'21');assert.equal(await p.locator('#usage-total-count').textContent(),'共 21 条');assert.equal(await p.locator('#usage-rows tr').count(),20);
 assert.equal(await p.locator('#usage-cost').textContent(),'2.100000');assert.equal(await p.locator('#usage-cache').textContent(),'80.0%');
 assert.match(await p.locator('#usage-key-count').textContent(),/2 个历史 Key/);assert.equal(await p.locator('#usage-rows [data-request-id="usage-0"]').count(),1);assert.equal(await p.locator('#usage-rows [data-request-id="usage-1"]').count(),1);
 const firstRow=p.locator('#usage-rows tr').first();assert.equal(await firstRow.locator('td').nth(3).textContent(),'0.100000');assert.equal(await firstRow.locator('td').nth(4).textContent(),'100');assert.match(await firstRow.locator('td').nth(7).textContent(),/成功 200/);
 const buckets=(await calls(p,'desktop_usage_buckets')).length;await p.locator('#usage-next').click();await p.locator('#usage-rows [data-request-id="usage-20"]').waitFor();
 assert.equal(await p.locator('#usage-rows tr').count(),1);assert.equal(await p.locator('#usage-total-count').textContent(),'共 21 条');assert.equal(await p.locator('#usage-requests').textContent(),'21');assert.equal((await calls(p,'desktop_usage_buckets')).length,buckets);
 assert.equal((await calls(p,'desktop_usage_logs')).at(-1).args.request.model,'');assert.equal(await p.locator('#usage-next').isDisabled(),true);
 await p.locator('#usage-prev').click();await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();assert.equal(await p.locator('#usage-rows tr').count(),20);
 await p.locator('#usage-page-size').selectOption('50');await p.locator('#usage-rows [data-request-id="usage-20"]').waitFor();assert.equal(await p.locator('#usage-rows tr').count(),21);assert.equal(await p.locator('#usage-total-count').textContent(),'共 21 条');assert.equal((await calls(p,'desktop_usage_buckets')).length,buckets);assert.equal(await p.locator('#usage-next').isDisabled(),true);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('scope changes discard old requests and clear private rows on logout',async()=>{
 const f=await open({deferUsage:true});try{const p=f.page;await p.waitForFunction(()=>window.__fixture.usageDeferred.length===2);
 await p.evaluate(async()=>{window.__fixture.current=window.__fixture.team;await window.TokenMarketWorkspace.refresh();});
 await p.waitForFunction(()=>window.__fixture.usageDeferred.length===4);
 await p.evaluate(()=>{const pending=window.__fixture.usageDeferred.splice(0);window.__fixture.oldUsage=pending.slice(0,2);pending.slice(2).forEach(resolve=>resolve());});
 await p.locator('#usage-rows [data-request-id="team-only"]').waitFor();
 // Resolve old personal requests last; they must not replace team data.
 await p.evaluate(async()=>{window.__fixture.oldUsage.forEach(resolve=>resolve());await new Promise(requestAnimationFrame);});
 assert.equal(await p.locator('#usage-rows [data-request-id="team-only"]').count(),1);assert.equal(await p.locator('#usage-rows [data-request-id="usage-0"]').count(),0);
 await p.evaluate(()=>{window.__fixture.emit('desktop-login-status',{context:null});});
 assert.equal(await p.locator('#usage-rows [data-request-id]').count(),0);assert.equal(await p.locator('#usage-cost').textContent(),'—');assert.equal(await p.locator('#usage-total-count').textContent(),'共 — 条');assert.deepEqual(f.errors,[]);
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
 const f=await open({usageSummaryError:true});try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();await p.locator('#usage-feedback').waitFor();assert.equal(await p.locator('#usage-cost').textContent(),'—');assert.equal(await p.locator('#usage-total-count').textContent(),'共 — 条');assert.equal(await p.locator('#usage-next').isEnabled(),true);assert.equal(await p.locator('#usage-refresh').isEnabled(),true);assert.deepEqual(f.errors,[]);}finally{await f.close();}
});

test('icon tools filter all models and statuses without the removed dropdowns',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 await p.evaluate(()=>{const row=window.__fixture.usageRows.find(r=>r.requestId==='usage-20');row.model='fixture-model-long';row.status='FAILED';row.statusCode=429;});
 await p.locator('#usage-tools [data-tool="codex"]').click();
 await p.waitForFunction(()=>!document.querySelector('#usage-refresh').disabled);
 assert.equal(await p.locator('#usage-scope-trigger,#usage-scope-panel,#usage-status').count(),0);
 assert.equal(await p.locator('#usage-tools [data-tool="codex"]').getAttribute('aria-pressed'),'true');
 assert.equal(await p.locator('#usage-requests').textContent(),'21');
 for(const command of ['desktop_usage_logs','desktop_usage_buckets']){const q=(await calls(p,command)).at(-1).args.request;assert.equal(q.tool,'codex');assert.equal(q.model,'');assert.equal(q.status,'ALL');}
 await p.locator('#usage-next').click();await p.locator('#usage-rows [data-request-id="usage-20"]').waitFor();assert.match(await p.locator('#usage-rows').textContent(),/失败 429/);
 await p.locator('#usage-tools [data-tool=""]').click();await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 assert.equal((await calls(p,'desktop_usage_logs')).at(-1).args.request.tool,'');assert.equal(await p.locator('#usage-requests').textContent(),'21');
 assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('request log heading shows full-range counts and metric cards switch the visible trend without refetching',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 const before=(await calls(p,'desktop_usage_buckets')).length;
 assert.equal(await p.locator('#usage-log-count').textContent(),'21');
 assert.equal(await p.locator('#usage-rows tr').count(),20);
 await p.locator('[data-usage-metric="requestCount"]').click();assert.equal(await p.locator('#usage-chart-title').textContent(),'请求数 · 按天');
 assert.equal(await p.locator('#usage-trend').isVisible(),true);
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('bounded date presets include exact 24 hours, 14 and 30 days; cancelling edits keeps the snapshot',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 for(const preset of ['24h','14d','30d']){
   await p.locator('#usage-range-trigger').click();await p.locator(`[data-range-preset="${preset}"]`).click();
   await p.waitForFunction(()=>!document.querySelector('#usage-refresh').disabled);
   const request=(await calls(p,'desktop_usage_buckets')).at(-1).args.request;
   if(preset==='24h')assert.equal(request.endTime-request.startTime,86400000);
   assert.ok(request.startTime>0);assert.ok(request.endTime-request.startTime<=30*86400000);
   if(preset==='14d')assert.equal(new Date(request.startTime+8*3600000).toISOString().slice(11),'00:00:00.000Z');
 }
 const before=(await calls(p,'desktop_usage_buckets')).length;
 await p.locator('#usage-range-trigger').click();await p.locator('#usage-start-time').fill('12:34');await p.locator('#usage-date-cancel').click();
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before);assert.equal(await p.locator('#usage-range-label').textContent(),'近 30 天');
 assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('custom Beijing time validates the range, supports calendar days and pins the end across pages',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 await p.locator('#usage-range-trigger').click();
 await p.locator('#usage-start-date').fill('2026-01-01');await p.locator('#usage-start-time').fill('00:00');
 await p.locator('#usage-end-date').fill('2025-12-31');await p.locator('#usage-date-apply').click();
 assert.match(await p.locator('#usage-date-error').textContent(),/不能早于/);assert.equal(await p.locator('#usage-date-panel').isVisible(),true);
 await p.locator('#usage-live-end').check();await p.locator('#usage-date-apply').click();
 await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 const initial=(await calls(p,'desktop_usage_logs')).at(-1).args.request;
 assert.equal(initial.startTime,Date.parse('2026-01-01T00:00:00+08:00'));
 await p.locator('#usage-next').click();await p.locator('#usage-rows [data-request-id="usage-20"]').waitFor();
 assert.equal((await calls(p,'desktop_usage_logs')).at(-1).args.request.endTime,initial.endTime);
 await p.locator('#usage-range-trigger').click();await p.locator('#usage-month-next').click();
 const day=p.locator('#usage-calendar-days button:not(.outside):not(:disabled)').first();const label=await day.getAttribute('aria-label');await day.click();assert.equal(await p.locator('#usage-start-date').inputValue(),label);
 await p.locator('#usage-date-panel').press('Escape');assert.equal(await p.locator('#usage-date-panel').isHidden(),true);
 assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('date overlays stay inside the desktop viewport at narrow widths',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 for(const width of [1280,900,700]){
   await p.setViewportSize({width,height:900});
   for(const [trigger,panel] of [['#usage-range-trigger','#usage-date-panel']]){
     await p.locator(trigger).click();const box=await p.locator(panel).boundingBox();assert.ok(box.x>=0&&box.x+box.width<=width,`${panel} at ${width}: ${JSON.stringify(box)}`);await p.locator(panel).press('Escape');
   }
 }
 assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('long custom ranges show monthly trends and compute cache rates from summed tokens',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 await p.locator('#usage-range-trigger').click();await p.locator('#usage-start-date').fill('2026-01-01');await p.locator('#usage-start-time').fill('00:00');await p.locator('#usage-live-end').check();await p.locator('#usage-date-apply').click();
 await p.locator('#usage-chart-title').filter({hasText:'Tokens · 按月'}).waitFor();
 await p.locator('[data-usage-metric="cacheRate"]').click();assert.match(await p.locator('#usage-chart').getAttribute('aria-label'),/按月/);
 assert.match(await p.locator('#usage-chart svg title').last().textContent(),/80.0%/);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('trend labels retain their proportions as the desktop window resizes',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-chart svg text').first().waitFor();
 const before=(await calls(p,'desktop_usage_buckets')).length;
 for(const width of [900,1280,1920,700]){
   await p.setViewportSize({width,height:900});
   await p.waitForFunction(()=>{const svg=document.querySelector('#usage-chart svg');return Math.abs(svg.viewBox.baseVal.width-svg.getBoundingClientRect().width)<1;});
   const scales=await p.locator('#usage-chart svg text').first().evaluate(label=>{
     const m=label.getScreenCTM();return {x:Math.hypot(m.a,m.b),y:Math.hypot(m.c,m.d)};
   });
   assert.ok(Math.abs(scales.x-scales.y)<0.02,`distorted trend label at ${width}px: ${JSON.stringify(scales)}`);
   assert.ok(Math.abs(scales.x-1)<0.02,`trend font scales with window width at ${width}px`);
 }
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before);
 assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('a rolling 24-hour range uses hourly slots across midnight including the final endpoint',async()=>{
 const f=await fixture(browser);try{const p=f.page;const end=Date.parse('2026-11-05T14:28:00+08:00');await p.clock.setFixedTime(end);
 await p.evaluate(end=>{const base=window.__fixture.usageRows[0],start=end-86400000;window.__fixture.usageRows=[start+60000,start+3600000,start+10*3600000,end].map((startedAt,i)=>({...base,requestId:'hour-'+i,startedAt}));},end);
 await p.locator('[data-page="usage"]').click();await p.locator('#usage-chart svg').waitFor();
 await p.locator('#usage-range-trigger').click();await p.locator('[data-range-preset="24h"]').click();
 await p.locator('#usage-chart-title').filter({hasText:'Tokens · 按小时'}).waitFor();
 assert.equal(await p.locator('#usage-chart svg rect').count(),24);assert.equal(await p.locator('#usage-requests').textContent(),'4');assert.equal(await p.locator('#usage-cost').textContent(),'0.400000');
 await p.locator('[data-usage-metric="requestCount"]').click();
 const tips=await p.locator('#usage-chart svg rect title').allTextContents();
 for(const index of [0,1,10,23])assert.match(tips[index],/请求数：1$/);
 assert.match(tips[2],/请求数：0$/);assert.match(tips[0],/2026-11-04 14:28/);assert.match(tips[23],/2026-11-05 14:28/);
 assert.ok((await p.locator('#usage-chart svg text').allTextContents()).includes('11/05'));
 const count=(await calls(p,'desktop_usage_buckets')).length;await p.locator('[data-usage-metric="cacheRate"]').click();assert.equal((await calls(p,'desktop_usage_buckets')).length,count);
 assert.equal(await p.locator('#usage-cache').textContent(),'80.0%');assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('today and custom ranges through 24 hours use hours, with partial final slots and a daily fallback above the boundary',async()=>{
 const f=await fixture(browser);try{const p=f.page;const end=Date.parse('2026-11-05T14:28:00+08:00');await p.clock.setFixedTime(end);
 await p.evaluate(end=>{window.__fixture.usageRows=[{...window.__fixture.usageRows[0],startedAt:end}];},end);
 await p.locator('[data-page="usage"]').click();await p.locator('#usage-chart svg').waitFor();
 await p.locator('#usage-range-trigger').click();await p.locator('[data-range-preset="today"]').click();await p.locator('#usage-chart-title').filter({hasText:'按小时'}).waitFor();assert.equal(await p.locator('#usage-chart svg rect').count(),15);
 for(const [day,startTime,count,unit] of [['2026-11-05','11:58',3,'小时'],['2026-11-05','14:00',1,'小时'],['2026-11-05','14:28',1,'小时'],['2026-11-04','14:27',2,'天']]){
   await p.locator('#usage-range-trigger').click();await p.locator('#usage-start-date').fill(day);await p.locator('#usage-start-time').fill(startTime);
   await p.locator('#usage-end-date').fill('2026-11-05');await p.locator('#usage-end-time').fill('14:28');await p.locator('#usage-date-apply').click();
   await p.waitForFunction(()=>!document.querySelector('#usage-refresh').disabled);
   assert.equal(await p.locator('#usage-chart-title').textContent(),`Tokens · 按${unit}`);assert.equal(await p.locator('#usage-chart svg rect').count(),count);
   assert.equal(await p.locator('#usage-requests').textContent(),'1');assert.equal(await p.locator('#usage-cache').textContent(),'80.0%');
 }
 assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('legacy daily responses do not fabricate an hourly distribution from the current log page',async()=>{
 const f=await open({usageLegacyBuckets:true});try{const p=f.page;await p.locator('#usage-chart svg').waitFor();
 await p.locator('#usage-range-trigger').click();await p.locator('[data-range-preset="24h"]').click();await p.getByText('小时趋势暂不可用，请稍后刷新。').waitFor();
 assert.equal(await p.locator('#usage-requests').textContent(),'21');assert.equal(await p.locator('#usage-chart svg').count(),0);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('idle usage never refreshes on a timer and the refresh button reads immediately',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();await p.clock.install();
 const before=(await calls(p,'desktop_usage_buckets')).length;await p.clock.fastForward(10*60*1000);
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before);assert.equal((await calls(p,'desktop_usage_logs')).length,before);
 await p.evaluate(()=>{window.__fixture.usageRows.push({...window.__fixture.usageRows[0],requestId:'manual-new',startedAt:Date.now()});});
 await p.locator('#usage-refresh').click();await p.getByText('共 22 条',{exact:true}).waitFor();
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before+1);assert.equal((await calls(p,'desktop_usage_logs')).length,before+1);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('usage shares lifecycle synchronization and refreshes on activation and page return within 30 seconds',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();
 const commands=['desktop_usage_logs','desktop_usage_buckets','refresh_desktop_session','desktop_workspace_context'];
 const before=await Promise.all(commands.map(command=>calls(p,command).then(rows=>rows.length)));
 await p.evaluate(()=>{window.__fixture.usageRows.push({...window.__fixture.usageRows[0],requestId:'activated-new',startedAt:Date.now()});window.dispatchEvent(new Event('focus'));document.dispatchEvent(new Event('visibilitychange'));window.__fixture.emit('desktop-activated');});
 await p.getByText('共 22 条',{exact:true}).waitFor();assert.deepEqual(await Promise.all(commands.map(command=>calls(p,command).then(rows=>rows.length))),before.map(n=>n+1));
 const updates=(await calls(p,'check_tool_updates')).length;
 await p.locator('.primary-nav [data-page="account"]').click();await p.waitForFunction(n=>window.__fixture.calls.filter(c=>c.command==='check_tool_updates').length===n+1,updates);
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before[1]+1);
 await p.evaluate(()=>{window.__fixture.usageRows.push({...window.__fixture.usageRows[0],requestId:'returned-new',startedAt:Date.now()});});
 await p.locator('.primary-nav [data-page="usage"]').click();await p.getByText('共 23 条',{exact:true}).waitFor();
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before[1]+2);assert.equal((await calls(p,'desktop_usage_logs')).length,before[0]+2);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('lifecycle events during slow usage queries merge into one follow-up without blocking other page synchronization',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"]').waitFor();await p.clock.install();
 const before=(await calls(p,'desktop_usage_buckets')).length,workspace=(await calls(p,'desktop_workspace_context')).length;
 await p.evaluate(()=>{window.__fixture.deferUsage=true;window.__fixture.emit('desktop-activated');});await p.clock.runFor(200);await p.waitForFunction(()=>window.__fixture.usageDeferred.length===2);
 await p.evaluate(()=>{window.__fixture.usageRows.push({...window.__fixture.usageRows[0],requestId:'slow-new',startedAt:Date.now()});window.dispatchEvent(new Event('focus'));document.dispatchEvent(new Event('visibilitychange'));window.__fixture.emit('desktop-activated');});await p.clock.runFor(200);
 assert.equal((await calls(p,'desktop_workspace_context')).length,workspace+2);assert.equal((await calls(p,'desktop_usage_buckets')).length,before+1);
 await p.evaluate(()=>{window.__fixture.deferUsage=false;window.__fixture.usageDeferred.splice(0).forEach(resolve=>resolve());});await p.getByText('共 22 条',{exact:true}).waitFor();
 assert.equal((await calls(p,'desktop_usage_buckets')).length,before+2);assert.equal((await calls(p,'desktop_usage_logs')).length,before+2);
 await p.clock.fastForward(60*1000);assert.equal((await calls(p,'desktop_usage_buckets')).length,before+2);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('clicking a request opens a right-hand detail drawer using exact recorded values without extra requests',async()=>{
 const f=await fixture(browser);try{const p=f.page;
 await p.evaluate(()=>{const r=window.__fixture.usageRows[0];r.requestId='request-detail-fixture';r.costMicro='9007199254740993';r.inputTokens=313677;r.outputTokens=589;r.cacheReadTokens=313472;r.cacheWriteTokens=123;r.totalTokens=314266;r.durationMs=24000;r.ttftMs=2200;});
 await p.locator('[data-page="usage"]').click();await p.locator('#usage-rows [data-request-id="request-detail-fixture"]').waitFor();
 const before=(await calls(p,'desktop_usage_logs')).length;await p.locator('#usage-rows [data-request-id="request-detail-fixture"] td').nth(2).click();
 const drawer=p.locator('#usage-request-detail');assert.equal(await drawer.isVisible(),true);
 const field=key=>p.locator(`[data-detail-field="${key}"]`);
 assert.equal(await field('requestId').textContent(),'request-detail-fixture');assert.equal(await field('inputTokens').textContent(),'313,677');assert.equal(await field('cacheReadTokens').textContent(),'313,472');assert.equal(await field('cacheWriteTokens').textContent(),'123');assert.equal(await field('totalTokens').textContent(),'314,266');
 assert.equal(await field('cost').textContent(),'9,007,199,254.740993');assert.equal(await field('ttft').textContent(),'2.20s');assert.equal(await field('duration').textContent(),'24.00s');assert.match(await field('speed').textContent(),/27.0 tok\/s/);
 assert.equal((await calls(p,'desktop_usage_logs')).length,before);const box=await drawer.boundingBox();assert.ok(Math.abs(box.x+box.width-900)<1);assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('request details support keyboard opening, Escape, focus restoration and outside dismissal',async()=>{
 const f=await open();try{const p=f.page;const trigger=p.locator('#usage-rows [data-request-id="usage-0"] button');await trigger.waitFor();await trigger.focus();await trigger.press('Enter');
 await p.locator('#usage-request-detail[open]').waitFor();assert.equal(await p.locator('[data-detail-field="ttft"]').textContent(),'—');assert.equal(await p.locator('[data-detail-field="speed"]').textContent(),'—');
 assert.equal(await p.locator('#usage-detail-close').evaluate(n=>n===document.activeElement),true);
 for(let i=0;i<3;i++){await p.keyboard.press('Tab');assert.equal(await p.evaluate(()=>document.querySelector('#usage-request-detail').contains(document.activeElement)),true);}
 await p.keyboard.press('Escape');assert.equal(await p.locator('#usage-request-detail').isVisible(),false);assert.equal(await trigger.evaluate(n=>n===document.activeElement),true);
 await trigger.click();await p.mouse.click(10,200);assert.equal(await p.locator('#usage-request-detail').isVisible(),false);assert.equal(await p.locator('#usage-detail-body').textContent(),'');assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('an open request survives event refreshes, updates its record, and clears immediately on workspace or login changes',async()=>{
 const f=await open();try{const p=f.page;await p.locator('#usage-rows [data-request-id="usage-0"] button').click();
 await p.evaluate(()=>{window.__fixture.usageRows[0].costMicro='250000';window.__fixture.emit('desktop-activated');});await p.locator('[data-detail-field="cost"]').filter({hasText:'0.250000'}).waitFor();assert.equal(await p.locator('#usage-request-detail').isVisible(),true);
 await p.locator('#usage-detail-close').click();assert.equal(await p.locator('#usage-rows [data-request-id="usage-0"] button').evaluate(n=>n===document.activeElement),true);
 await p.locator('#usage-rows [data-request-id="usage-0"] button').click();await p.evaluate(async()=>{window.__fixture.current=window.__fixture.team;await window.TokenMarketWorkspace.refresh();});
 assert.equal(await p.locator('#usage-request-detail').isVisible(),false);assert.equal(await p.locator('#usage-detail-body').textContent(),'');
 await p.locator('#usage-rows [data-request-id="team-only"] button').waitFor();await p.locator('#usage-rows [data-request-id="team-only"] button').click();assert.equal(await p.locator('[data-detail-field="statusCode"]').textContent(),'429');
 await p.evaluate(()=>{window.__fixture.emit('desktop-login-status',{context:null});});assert.equal(await p.locator('#usage-request-detail').isVisible(),false);assert.equal(await p.locator('#usage-detail-body').textContent(),'');assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});
test('request detail strings stay escaped and long IDs wrap inside light and dark narrow drawers',async()=>{
 for(const colorScheme of ['light','dark']){
 const f=await fixture(browser,{colorScheme,usageModel:'<img src=x onerror=alert(1)>'});try{const p=f.page;
 const requestId='request-'+('abc123'.repeat(35))+'<img src=x onerror=alert(1)>';await p.evaluate(requestId=>{window.__fixture.usageRows[0].requestId=requestId;},requestId);
 await p.locator('[data-page="usage"]').click();await p.locator('#usage-rows tr button').first().click();assert.equal(await p.locator('#usage-detail-body img').count(),0);assert.equal(await p.locator('[data-detail-field="model"]').textContent(),'<img src=x onerror=alert(1)>');assert.equal(await p.locator('[data-detail-field="requestId"]').textContent(),requestId);
 for(const width of [1280,700,420]){await p.setViewportSize({width,height:760});const box=await p.locator('#usage-request-detail').boundingBox();assert.ok(box.x>=0&&box.x+box.width<=width);assert.equal(await p.locator('#usage-detail-body').evaluate(n=>n.scrollWidth>n.clientWidth),false);}
 await p.evaluate(()=>window.TokenMarketShell.show('tools'));assert.equal(await p.locator('#usage-request-detail').isVisible(),false);assert.equal(await p.locator('#usage-detail-body').textContent(),'');assert.deepEqual(f.errors,[]);
 }finally{await f.close();}}
});

test('calendar selection stays visible and editable when the end follows now',async()=>{
 for(const colorScheme of ['dark','light']){
 const f=await fixture(browser,{colorScheme});try{const p=f.page;await p.clock.setFixedTime(Date.parse('2026-11-05T14:35:00+08:00'));
 await p.locator('[data-page="usage"]').click();await p.locator('#usage-rows').waitFor();await p.locator('#usage-range-trigger').click();
 await p.locator('#usage-start-date').fill('2026-10-14');await p.locator('#usage-start-time').fill('23:00');await p.locator('#usage-calendar-days [aria-label="2026-10-14"]').click();await p.locator('#usage-live-end').check();
 const selected=p.locator('#usage-calendar-days [aria-label="2026-10-14"]');
 const contrast=await selected.evaluate(button=>{
   const rgb=value=>value.match(/[\d.]+/g).map(Number);
   const luminance=rgb=>rgb.slice(0,3).map(v=>{v/=255;return v<=.04045?v/12.92:((v+.055)/1.055)**2.4;}).reduce((n,v,i)=>n+v*[.2126,.7152,.0722][i],0);
   const style=getComputedStyle(button);let bg=rgb(style.backgroundColor),ancestor=button.parentElement;
   while(bg.length===4&&bg[3]===0&&ancestor){bg=rgb(getComputedStyle(ancestor).backgroundColor);ancestor=ancestor.parentElement;}
   const a=luminance(rgb(style.color)),b=luminance(bg);return (Math.max(a,b)+.05)/(Math.min(a,b)+.05);
 });
 assert.ok(contrast>=4.5,`${colorScheme} selected date contrast: ${contrast}`);
 assert.equal(await selected.isEnabled(),true);assert.equal(await p.locator('#usage-calendar-days [aria-label="2026-10-15"]').isEnabled(),true);assert.equal(await p.locator('#usage-calendar-days [aria-label="2026-11-06"]').isDisabled(),true);
 await p.locator('#usage-calendar-days [aria-label="2026-10-15"]').click();assert.equal(await p.locator('#usage-start-date').inputValue(),'2026-10-15');assert.equal(await p.locator('#usage-live-end').isChecked(),true);
 await p.locator('#usage-live-end').uncheck();await p.locator('[data-date-field="end"]').click();await p.locator('#usage-calendar-days [aria-label="2026-10-20"]').click();assert.equal(await p.locator('#usage-end-date').inputValue(),'2026-10-20');assert.deepEqual(f.errors,[]);
 }finally{await f.close();}}
});
