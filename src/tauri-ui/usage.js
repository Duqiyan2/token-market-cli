/* Platform facts are fetched with native-owned identity; charts use server-aggregated buckets. */
(() => {
  const el = id => document.getElementById(id);
  const root = el('usage-page');
  if (!root) return;
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const node = (tag, value, className) => { const n = document.createElement(tag); n.textContent = value ?? ''; if (className) n.className = className; return n; };
  const number = value => Number(value || 0).toLocaleString('en-US');
  const compact = value => Intl.NumberFormat('en-US', {notation:'compact',maximumFractionDigits:2}).format(Number(value || 0));
  const integer = value => { try { return BigInt(value || '0'); } catch (_) { return 0n; } };
  function money(value) { const n = integer(value), a = n < 0n ? -n : n; return `${n < 0n ? '-' : ''}${(a / 1000000n).toLocaleString('en-US')}.${(a % 1000000n).toString().padStart(6, '0')}`; }
  const date = value => new Intl.DateTimeFormat('en-CA', {timeZone:'Asia/Shanghai',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date(value));
  const time = value => new Intl.DateTimeFormat('zh-CN', {timeZone:'Asia/Shanghai',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',second:'2-digit',hour12:false}).format(new Date(value));
  const clock = value => new Intl.DateTimeFormat('en-GB', {timeZone:'Asia/Shanghai',hour:'2-digit',minute:'2-digit',hourCycle:'h23'}).format(new Date(value));
  const duration = ms => ms == null || ms <= 0 ? '—' : ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${Math.round(ms)}ms`;
  const fields = ['requestCount','inputTokens','outputTokens','totalTokens','cacheReadTokens','cacheWriteTokens','inputTokenCount'];
  const metricNames = {costMicro:'费用',requestCount:'请求数',totalTokens:'Tokens',cacheRate:'缓存命中率'};
  let context = null, generation = 0, metric = 'totalTokens', tools = [], buckets = null, detail = null;
  let cursors = [null], pageIndex = 0, snapshot = null, loadingBuckets = false, loadingLogs = false;
  let summaryError = '', detailError = '', syncRequested = false;
  let selectedTool = '', chartWidth = 0;
  const visible = () => context && window.TokenMarketShell?.page === 'usage' && !document.hidden;
  function scopeKey() { return context ? `${context.user.id}:${context.default_workspace.id}` : ''; }
  const range = () => window.TokenMarketUsageRange.read();
  function filters() { return {tool:selectedTool, model:'', status:'ALL'}; }
  function query(cursor = null, page = 1) { return {...snapshot,ownerId:String(context.user.id),workspaceId:String(context.default_workspace.id),cursor,page,pageSize:Number(el('usage-page-size').value)}; }
  function emptyRow(message) { const row = node('tr'), cell = node('td',message,'usage-empty'); cell.colSpan = 10;row.append(cell);el('usage-rows').replaceChildren(row); }
  function controls() {
    el('usage-refresh').disabled = loadingBuckets || loadingLogs;
    el('usage-page-size').disabled = loadingLogs;
    root.querySelector('.usage-overview').setAttribute('aria-busy',String(loadingBuckets));
    el('usage-rows').setAttribute('aria-busy',String(loadingLogs));
    el('usage-prev').disabled = loadingLogs || pageIndex === 0 || !detail;
    el('usage-next').disabled = loadingLogs || !detail?.nextCursor;
    el('usage-page-label').textContent = `第 ${pageIndex + 1} 页`;
    el('usage-updated').textContent = loadingBuckets ? '正在读取汇总…' : buckets ? `更新于 ${time(buckets.asOf)}` : '';
    const errors = [summaryError,detailError].filter(Boolean);el('usage-feedback').textContent = errors.join('；');el('usage-feedback').hidden = errors.length === 0;
  }
  function toolOptions(next) {
    const focusedTool=el('usage-tools').contains(document.activeElement)?document.activeElement.dataset.tool:null;
    tools = next || [];
    el('usage-key-count').textContent = `${tools.length} 个工具 · ${number(tools.reduce((sum,t)=>sum+t.keyCount,0))} 个历史 Key`;
    const choices=[{id:'',name:'全部'},...tools];
    if(selectedTool&&!tools.some(t=>t.id===selectedTool))choices.push({id:selectedTool,name:selectedTool});
    el('usage-tools').replaceChildren(...choices.map(tool=>{
      const button=node('button');button.type='button';button.dataset.tool=tool.id;button.title=tool.name;button.setAttribute('aria-label',tool.name);button.setAttribute('aria-pressed',String(selectedTool===tool.id));
      if(!tool.id)button.textContent='全部';
      else {
        const image=document.createElement('img'),ext=({'workbuddy':'svg',openclaw:'svg',codex:'ico','deepseek-harness':'ico','grok-build':'svg',hermes:'svg',pi:'svg','minimax-code':'svg','oh-my-pi':'svg','kimi-code':'svg'})[tool.id] || 'png';
        // Tool IDs are server-owned; an unknown future tool gets a text fallback.
        if(/^[a-z0-9-]+$/.test(tool.id)){image.src=`assets/tools/${tool.id==='dumate-enterprise'?'dumate':tool.id}.${ext}`;image.alt='';image.onerror=()=>image.replaceWith(node('span',tool.name.slice(0,2)));button.append(image);}else button.textContent=tool.name.slice(0,2);
      }
      button.onclick=()=>selectTool(tool.id);return button;
    }));
    if(focusedTool!=null)[...el('usage-tools').children].find(b=>b.dataset.tool===focusedTool)?.focus();
  }
  function selectTool(tool) {selectedTool=tool;toolOptions(tools);void refresh(true);}
  function totals(list) {
    const total = Object.fromEntries(fields.map(key=>[key,0]));total.costMicro=0n;
    for (const b of list) { for (const key of fields) total[key]+=Number(b[key] || 0);total.costMicro+=integer(b.costMicro); }
    return total;
  }
  function metricValue(b) { return metric === 'cacheRate' ? b.inputTokenCount > 0 ? b.cacheReadTokens / b.inputTokenCount * 100 : 0 : metric === 'costMicro' ? Number(integer(b.costMicro)) / 1000000 : Number(b[metric] || 0); }
  function renderSummary() {
    const count=buckets?number(totals(buckets.buckets).requestCount):'—';
    el('usage-log-count').textContent=count;
    const totalLabel=`共 ${count} 条`;if(el('usage-total-count').textContent!==totalLabel)el('usage-total-count').textContent=totalLabel;
    if (!buckets) { el('usage-tokens').title='';el('usage-chart').setAttribute('aria-label','暂无汇总数据');['usage-cost','usage-requests','usage-tokens','usage-cache'].forEach(id=>{el(id).textContent='—';});el('usage-chart').replaceChildren(node('p',loadingBuckets?'正在读取用量…':'暂无汇总数据'));return; }
    const total = totals(buckets.buckets);
    el('usage-cost').textContent=money(total.costMicro);el('usage-requests').textContent=number(total.requestCount);el('usage-tokens').textContent=compact(total.totalTokens);
    el('usage-tokens').title=number(total.totalTokens);el('usage-cache').textContent=total.inputTokenCount>0?`${(total.cacheReadTokens/total.inputTokenCount*100).toFixed(1)}%`:'—';
    const hourly=snapshot.endTime-snapshot.startTime<=86400000;
    el('usage-chart-title').textContent=`${metricNames[metric]} · 按${hourly?'小时':'天'}`;
    if(!total.requestCount) { el('usage-chart').setAttribute('aria-label','所选时间内暂无用量');el('usage-chart').replaceChildren(node('p','所选时间内暂无用量'));return; }
    let series=[];
    if(hourly) {
      if(buckets.granularity!=='HOUR' || buckets.buckets.some(b=>!Number.isFinite(b.bucketStart))) {
        el('usage-chart').setAttribute('aria-label','小时趋势暂不可用');el('usage-chart').replaceChildren(node('p','小时趋势暂不可用，请稍后刷新。'));return;
      }
      const byHour=new Map(buckets.buckets.map(b=>[b.bucketStart,b]));
      const count=Math.max(1,Math.ceil((snapshot.endTime-snapshot.startTime)/3600000));
      for(let i=0;i<count;i++) {
        const bucketStart=snapshot.startTime+i*3600000;
        series.push({date:date(bucketStart),bucketStart,bucketEnd:Math.min(bucketStart+3600000,snapshot.endTime),...(byHour.get(bucketStart)||{})});
      }
    } else {
      const byDate=new Map(buckets.buckets.map(b=>[b.date,b]));
      const firstDay=Date.parse(`${date(snapshot.startTime)}T00:00:00+08:00`);
      for(let t=firstDay;t<=snapshot.endTime;t+=86400000) {const key=date(t);series.push({date:key,...(byDate.get(key)||{})});}
    }
    const monthly=!hourly&&series.length>90;
    if(monthly){const months=new Map();for(const b of series){const key=b.date.slice(0,7);if(!months.has(key))months.set(key,[]);months.get(key).push(b);}series=[...months].map(([key,list])=>({date:key,...totals(list)}));el('usage-chart-title').textContent=`${metricNames[metric]} · 按月`;}
    const ns='http://www.w3.org/2000/svg';
    const svgNode=(name,attrs={})=>{const n=document.createElementNS(ns,name);for(const [key,value] of Object.entries(attrs))n.setAttribute(key,String(value));return n;};
    const chart=el('usage-chart'),style=getComputedStyle(chart);
    const width=Math.max(1,chart.clientWidth-parseFloat(style.paddingLeft)-parseFloat(style.paddingRight));
    chartWidth=width;
    const height=230,left=58,right=16,top=16,baseline=194,plotWidth=Math.max(1,width-left-right),plotHeight=baseline-top;
    const svg=svgNode('svg',{viewBox:`0 0 ${width} ${height}`,'aria-hidden':'true',preserveAspectRatio:'xMinYMin meet'}),max=Math.max(...series.map(metricValue),1);
    for(let i=0;i<3;i++) {
      const y=top+i*plotHeight/2;
      svg.append(svgNode('line',{x1:left,x2:width-right,y1:y,y2:y,class:'usage-gridline'}));
      const label=svgNode('text',{x:left-12,y:y+4,'text-anchor':'end'});
      label.textContent=metric==='cacheRate'?`${(max*(1-i/2)).toFixed(0)}%`:compact(max*(1-i/2));svg.append(label);
    }
    const step=plotWidth/series.length,barWidth=Math.min(36,step*.65);
    const labelCount=Math.min(series.length,Math.max(2,Math.floor(plotWidth/85)));
    const labelIndices=new Set(Array.from({length:labelCount},(_,i)=>labelCount===1?0:Math.round(i*(series.length-1)/(labelCount-1))));
    if(hourly)series.forEach((b,i)=>{if(i>0&&b.date!==series[i-1].date){for(const index of labelIndices)if(Math.abs(index-i)*step<55)labelIndices.delete(index);labelIndices.add(i);}});
    series.forEach((b,i)=>{
      const value=metricValue(b),barHeight=value/max*plotHeight,center=left+(i+.5)*step;
      const bar=svgNode('rect',{x:center-barWidth/2,y:baseline-barHeight,width:barWidth,height:barHeight,rx:3});
      const title=svgNode('title');title.textContent=`${hourly?`${b.date} ${clock(b.bucketStart)} – ${date(b.bucketEnd)} ${clock(b.bucketEnd)}`:b.date} ${metricNames[metric]}：${metric==='costMicro'?money(b.costMicro):metric==='cacheRate'?`${value.toFixed(1)}%`:number(value)}`;
      bar.append(title);svg.append(bar);
      if(labelIndices.has(i)) {
        const label=svgNode('text',{x:Math.min(width-right-18,Math.max(left+18,center)),y:220,'text-anchor':'middle'});
        label.textContent=hourly?(i>0&&b.date!==series[i-1].date?b.date.slice(5).replace('-','/'):clock(b.bucketStart)):monthly?b.date.slice(2).replace('-','/'):b.date.slice(5).replace('-','/');svg.append(label);
      }
    });
    el('usage-chart').setAttribute('aria-label',`${metricNames[metric]}按${hourly?'小时':monthly?'月':'日'}趋势，${series.length} ${hourly?'个小时':monthly?'个月':'天'}`);el('usage-chart').replaceChildren(svg);
  }
  function renderRows() {
    if (!detail) { emptyRow(loadingLogs?'正在读取请求日志…':detailError?'请求日志读取失败，请重试。':'暂无请求日志');return; }
    if(!detail.items.length) { emptyRow(tools.length?'没有符合条件的请求日志。':'本设备在当前空间还没有工具用量。');return; }
    const names=new Map(tools.map(t=>[t.id,t.name]));
    el('usage-rows').replaceChildren(...detail.items.map(item=>{
      const row=node('tr');row.dataset.requestId=item.requestId;row.className='usage-request-row';row.title=`查看请求详情 · 请求 ID：${item.requestId}`;
      const values=[time(item.startedAt),names.get(item.tool)||item.tool||'未知工具',item.model||'—',money(item.costMicro),number(item.inputTokens),number(item.outputTokens),number(item.cacheReadTokens)];
      values.forEach((value,index)=>{const cell=node('td',index===0?'':value,index===2?'usage-model-cell':'');if(index===0){const button=node('button',value,'usage-request-open');button.type='button';button.setAttribute('aria-label',`查看 ${time(item.startedAt)} 的请求详情`);button.setAttribute('aria-haspopup','dialog');button.setAttribute('aria-controls','usage-request-detail');cell.append(button);}if(index===2)cell.title=value;row.append(cell);});
      const status=node('td'),cancelled=item.status?.startsWith('CLIENT_CANCELLED'),success=item.status==='SUCCESS';const badge=node('span',`${success?'成功':cancelled?'已取消':'失败'}${item.statusCode>0?` ${item.statusCode}`:''}`,`usage-status ${success?'success':cancelled?'':'failed'}`);status.append(badge);row.append(status,node('td',duration(item.durationMs)),node('td',duration(item.ttftMs)));row.onclick=event=>{if(event.target.closest('button')||!window.getSelection()?.toString())window.TokenMarketUsageDetail.open(item,{toolName:names.get(item.tool)||item.tool||'未知工具',workspace:context.default_workspace.name},row.querySelector('button'));};return row;
    }));
    window.TokenMarketUsageDetail.update(detail.items,{tools,workspace:context.default_workspace.name});
  }
  function invalidate(clearScope = false) {
    generation++;loadingBuckets=false;loadingLogs=false;buckets=null;detail=null;cursors=[null];pageIndex=0;snapshot=null;summaryError='';detailError='';syncRequested=false;
    if(clearScope){window.TokenMarketUsageDetail.close(false);tools=[];selectedTool='';toolOptions([]);window.TokenMarketUsageRange.close();}
    renderSummary();renderRows();controls();
  }
  function flushSync() {
    if(syncRequested&&!loadingBuckets&&!loadingLogs){syncRequested=false;void refresh();}
  }
  async function loadPage(target) {
    if(!visible() || !snapshot || loadingLogs)return;
    const gen=generation,request=query(cursors[target],target+1);loadingLogs=true;detailError='';controls();if(!detail)renderRows();
    try { const result=await invoke('desktop_usage_logs',{request});if(gen!==generation)return;detail=result;pageIndex=target;toolOptions(result.tools);renderRows(); }
    catch(error){if(gen===generation){detailError=`日志：${String(error)}`;if(!detail)renderRows();}}
    finally{if(gen===generation){loadingLogs=false;controls();flushSync();}}
  }
  async function refresh(force = false) {
    if(!visible())return;
    if(!force && (loadingBuckets||loadingLogs)){syncRequested=true;return;}
    invalidate();snapshot={...range(),...filters()};
    const gen=generation,request=query();
    loadingBuckets=true;renderSummary();controls();
    const summary = (async()=>{
      try {
        const result=await invoke('desktop_usage_buckets',{request});
        if(gen!==generation)return;
        buckets=result;toolOptions(result.tools);summaryError='';renderSummary();
      }catch(error){if(gen===generation){summaryError=`汇总：${String(error)}`;renderSummary();}}
      finally{if(gen===generation){loadingBuckets=false;renderSummary();controls();flushSync();}}
    })();
    await Promise.allSettled([summary,loadPage(0)]);
  }
  el('usage-filters').onsubmit=event=>{event.preventDefault();if(!el('usage-date-panel').hidden)el('usage-date-apply').click();else void refresh(true);};
  window.addEventListener('usage-range-changed',()=>void refresh(true));
  toolOptions([]);
  el('usage-refresh').onclick=()=>void refresh(true);
  el('usage-page-size').onchange=()=>{cursors=[null];pageIndex=0;detail=null;void loadPage(0);};
  el('usage-next').onclick=()=>{if(detail?.nextCursor){cursors[pageIndex+1]=detail.nextCursor;void loadPage(pageIndex+1);}};
  el('usage-prev').onclick=()=>{if(pageIndex>0)void loadPage(pageIndex-1);};
  document.querySelectorAll('[data-usage-metric]').forEach(button=>{button.onclick=()=>{metric=button.dataset.usageMetric;document.querySelectorAll('[data-usage-metric]').forEach(b=>{b.classList.toggle('active',b===button);b.setAttribute('aria-pressed',String(b===button));});renderSummary();};});
  window.addEventListener('current-workspace-changed',event=>{
    if(context){context={...context,default_workspace:event.detail};invalidate(true);el('usage-scope').textContent=`${event.detail.name} · 本设备工具的历史用量`;}
  });
  // Only resize the drawing; window changes must not refetch usage.
  const chartObserver=new ResizeObserver(entries=>{
    const width=Math.round(entries[0].contentRect.width);
    if(width>0 && width!==chartWidth && visible() && buckets)renderSummary();
  });
  chartObserver.observe(el('usage-chart'));
  window.TokenMarketUsage={refresh,setAuthenticated(next){
    const before=scopeKey(),workspace=window.TokenMarketWorkspace?.current?.current_workspace;
    context=next?{...next,default_workspace:workspace||next.default_workspace}:null;
    if(before!==scopeKey()){invalidate(true);el('usage-scope').textContent=context?`${context.default_workspace.name} · 本设备工具的历史用量`:'当前空间 · 本设备工具的历史用量';}
  }};
})();
