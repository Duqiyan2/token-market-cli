/* Platform facts are fetched with native-owned identity; the page only merges daily values. */
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
  const duration = ms => ms == null || ms <= 0 ? '—' : ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${ms}ms`;
  const fields = ['requestCount','inputTokens','outputTokens','totalTokens','cacheReadTokens','cacheWriteTokens','inputTokenCount'];
  const metricNames = {costMicro:'费用',requestCount:'请求数',totalTokens:'Tokens',cacheRate:'缓存命中率'};
  let context = null, generation = 0, metric = 'totalTokens', tools = [], buckets = null, detail = null;
  let cursors = [null], pageIndex = 0, snapshot = null, loadingBuckets = false, loadingLogs = false;
  let summaryError = '', detailError = '', lastRead = 0, cached = new Map();
  const visible = () => context && window.TokenMarketShell?.page === 'usage' && !document.hidden;
  function scopeKey() { return context ? `${context.user.id}:${context.default_workspace.id}` : ''; }
  function range() { const endTime = Date.now(), days = Number(el('usage-range').value); const start = new Date(`${date(endTime)}T00:00:00+08:00`).getTime(); return {startTime:start - (days - 1) * 86400000,endTime}; }
  function filters() { return {tool:el('usage-tool').value, model:el('usage-model').value.trim(), status:el('usage-status').value}; }
  function query(cursor = null, page = 1) { return {...snapshot,ownerId:String(context.user.id),workspaceId:String(context.default_workspace.id),cursor,page,pageSize:Number(el('usage-page-size').value)}; }
  function emptyRow(message) { const row = node('tr'), cell = node('td',message,'usage-empty'); cell.colSpan = 11;row.append(cell);el('usage-rows').replaceChildren(row); }
  function controls() {
    el('usage-refresh').disabled = loadingBuckets || loadingLogs;
    el('usage-page-size').disabled = loadingLogs;
    el('usage-rows').setAttribute('aria-busy',String(loadingLogs));
    el('usage-prev').disabled = loadingLogs || pageIndex === 0 || !detail;
    el('usage-next').disabled = loadingLogs || !detail?.nextCursor;
    el('usage-page-label').textContent = `第 ${pageIndex + 1} 页`;
    el('usage-updated').textContent = loadingBuckets ? '正在读取汇总…' : buckets ? `更新于 ${time(buckets.asOf)}` : '';
    const errors = [summaryError,detailError].filter(Boolean);el('usage-feedback').textContent = errors.join('；');el('usage-feedback').hidden = errors.length === 0;
  }
  function toolOptions(next) {
    tools = next || [];
    const selected = el('usage-tool').value;
    const first = node('option','全部工具');first.value = '';
    const options = tools.map(tool => { const o = node('option',tool.name);o.value=tool.id;return o; });
    el('usage-tool').replaceChildren(first,...options);
    // Retain a selected tool when no records match the current date/model/status filters.
    if (selected && !tools.some(t => t.id === selected)) { const option = node('option',selected);option.value=selected;el('usage-tool').append(option); }
    el('usage-tool').value = selected;
    el('usage-key-count').textContent = `${tools.length} 个工具 · ${number(tools.reduce((sum,t)=>sum+t.keyCount,0))} 个历史 Key`;
  }
  function totals(list) {
    const total = Object.fromEntries(fields.map(key=>[key,0]));total.costMicro=0n;
    for (const b of list) { for (const key of fields) total[key]+=Number(b[key] || 0);total.costMicro+=integer(b.costMicro); }
    return total;
  }
  function metricValue(b) { return metric === 'cacheRate' ? b.inputTokenCount > 0 ? b.cacheReadTokens / b.inputTokenCount * 100 : 0 : metric === 'costMicro' ? Number(integer(b.costMicro)) / 1000000 : Number(b[metric] || 0); }
  function renderSummary() {
    if (!buckets) { ['usage-cost','usage-requests','usage-tokens','usage-cache'].forEach(id=>{el(id).textContent='—';});el('usage-chart').replaceChildren(node('p',loadingBuckets?'正在读取用量…':'暂无汇总数据'));return; }
    const total = totals(buckets.buckets);
    el('usage-cost').textContent=money(total.costMicro);el('usage-requests').textContent=number(total.requestCount);el('usage-tokens').textContent=compact(total.totalTokens);
    el('usage-tokens').title=number(total.totalTokens);el('usage-cache').textContent=total.inputTokenCount>0?`${(total.cacheReadTokens/total.inputTokenCount*100).toFixed(1)}%`:'—';
    el('usage-chart-title').textContent=`${metricNames[metric]} · 按天`;
    const byDate = new Map(buckets.buckets.map(b=>[b.date,b]));
    const series = [];
    for(let t=snapshot.startTime;t<=snapshot.endTime;t+=86400000) { const key=date(t);series.push({date:key,...(byDate.get(key)||{})}); }
    if(!total.requestCount) { el('usage-chart').replaceChildren(node('p','所选时间内暂无用量'));return; }
    const ns='http://www.w3.org/2000/svg';
    const svgNode=(name,attrs={})=>{const n=document.createElementNS(ns,name);for(const [key,value] of Object.entries(attrs))n.setAttribute(key,String(value));return n;};
    const svg=svgNode('svg',{viewBox:'0 0 720 210','aria-hidden':'true'}),max=Math.max(...series.map(metricValue),1);
    for(let i=0;i<3;i++) { const y=20+i*75;svg.append(svgNode('line',{x1:55,x2:708,y1:y,y2:y,class:'usage-gridline'}));const label=svgNode('text',{x:47,y:y+4,'text-anchor':'end'});label.textContent=metric==='cacheRate'?`${(max*(1-i/2)).toFixed(0)}%`:compact(max*(1-i/2));svg.append(label); }
    const step=650/series.length;
    series.forEach((b,i)=>{const value=metricValue(b),height=value/max*150,x=58+i*step;const bar=svgNode('rect',{x,y:170-height,width:Math.max(2,step*.65),height,rx:3});const title=svgNode('title');title.textContent=`${b.date} ${metricNames[metric]}：${metric==='costMicro'?money(b.costMicro):metric==='cacheRate'?`${value.toFixed(1)}%`:number(value)}`;bar.append(title);svg.append(bar);
      if(i%Math.max(1,Math.ceil(series.length/7))===0 || i===series.length-1) { const label=svgNode('text',{x:x+step*.3,y:194,'text-anchor':'middle'});label.textContent=b.date.slice(5).replace('-','/');svg.append(label); }
    });
    el('usage-chart').setAttribute('aria-label',`${metricNames[metric]}按日趋势，${series.length} 天`);el('usage-chart').replaceChildren(svg);
  }
  function renderRows() {
    if (!detail) { emptyRow(loadingLogs?'正在读取请求日志…':detailError?'请求日志读取失败，请重试。':'暂无请求日志');return; }
    if(!detail.items.length) { emptyRow(tools.length?'没有符合条件的请求日志。':'本设备在当前空间还没有工具用量。');return; }
    const names=new Map(tools.map(t=>[t.id,t.name]));
    el('usage-rows').replaceChildren(...detail.items.map(item=>{
      const row=node('tr');row.dataset.requestId=item.requestId;row.title=`请求 ID：${item.requestId}`;
      const values=[time(item.startedAt),names.get(item.tool)||item.tool||'未知工具',item.model||'—',item.keyName||item.keyId||'—',money(item.costMicro),number(item.inputTokens),number(item.outputTokens),number(item.cacheReadTokens)];
      values.forEach((value,index)=>{const cell=node('td',value,index===2?'usage-model-cell':index===3?'usage-key':'');if(index===2||index===3)cell.title=index===3?`${value}（ID：${item.keyId || '—'}）`:value;row.append(cell);});
      const status=node('td'),cancelled=item.status?.startsWith('CLIENT_CANCELLED'),success=item.status==='SUCCESS';const badge=node('span',`${success?'成功':cancelled?'已取消':'失败'}${item.statusCode>0?` ${item.statusCode}`:''}`,`usage-status ${success?'success':cancelled?'':'failed'}`);status.append(badge);row.append(status,node('td',duration(item.durationMs)),node('td',duration(item.ttftMs)));return row;
    }));
  }
  function invalidate(clearCache = false) {
    generation++;loadingBuckets=false;loadingLogs=false;buckets=null;detail=null;cursors=[null];pageIndex=0;snapshot=null;summaryError='';detailError='';lastRead=0;
    if(clearCache){cached.clear();tools=[];toolOptions([]);el('usage-tool').value='';}
    renderSummary();renderRows();controls();
  }
  async function loadPage(target) {
    if(!visible() || !snapshot || loadingLogs)return;
    const gen=generation,request=query(cursors[target],target+1);loadingLogs=true;detailError='';controls();if(!detail)renderRows();
    try { const result=await invoke('desktop_usage_logs',{request});if(gen!==generation)return;detail=result;pageIndex=target;toolOptions(result.tools);renderRows(); }
    catch(error){if(gen===generation){detailError=`日志：${String(error)}`;if(!detail)renderRows();}}
    finally{if(gen===generation){loadingLogs=false;controls();}}
  }
  async function refresh(force = false) {
    if(!visible())return;
    if(!force && (loadingBuckets||loadingLogs||Date.now()-lastRead<30000))return;
    invalidate();snapshot={...range(),...filters()};lastRead=Date.now();
    const gen=generation,request=query(),key=JSON.stringify({scope:scopeKey(),days:el('usage-range').value,...filters()}),existing=cached.get(key);
    loadingBuckets=true;renderSummary();controls();
    const summary = (async()=>{
      try {
        let result;
        if(!force && existing && Date.now()-existing.at<30000){result=existing.data;snapshot={...existing.range};}
        else result=await invoke('desktop_usage_buckets',{request});
        if(gen!==generation)return;
        buckets=result;toolOptions(result.tools);summaryError='';cached.set(key,{at:Date.now(),data:result,range:{...snapshot}});if(cached.size>12)cached.delete(cached.keys().next().value);renderSummary();
      }catch(error){if(gen===generation){summaryError=`汇总：${String(error)}`;renderSummary();}}
      finally{if(gen===generation){loadingBuckets=false;controls();}}
    })();
    await Promise.allSettled([summary,loadPage(0)]);
  }
  el('usage-filters').onsubmit=event=>{event.preventDefault();void refresh(true);};
  ['usage-range','usage-tool','usage-status'].forEach(id=>{el(id).onchange=()=>void refresh(true);});
  el('usage-refresh').onclick=()=>void refresh(true);
  el('usage-page-size').onchange=()=>{cursors=[null];pageIndex=0;detail=null;void loadPage(0);};
  el('usage-next').onclick=()=>{if(detail?.nextCursor){cursors[pageIndex+1]=detail.nextCursor;void loadPage(pageIndex+1);}};
  el('usage-prev').onclick=()=>{if(pageIndex>0)void loadPage(pageIndex-1);};
  document.querySelectorAll('[data-usage-metric]').forEach(button=>{button.onclick=()=>{metric=button.dataset.usageMetric;document.querySelectorAll('[data-usage-metric]').forEach(b=>{b.classList.toggle('active',b===button);b.setAttribute('aria-pressed',String(b===button));});renderSummary();};});
  window.addEventListener('tokenmarket-page-changed',event=>{if(event.detail.to==='usage')void refresh();});
  window.addEventListener('current-workspace-changed',event=>{
    if(context){context={...context,default_workspace:event.detail};invalidate(true);el('usage-scope').textContent=`${event.detail.name} · 本设备工具的历史用量`;void refresh();}
  });
  window.TokenMarketUsage={refresh,setAuthenticated(next){
    const before=scopeKey(),workspace=window.TokenMarketWorkspace?.current?.current_workspace;
    context=next?{...next,default_workspace:workspace||next.default_workspace}:null;
    if(before!==scopeKey()){invalidate(true);el('usage-model').value='';el('usage-status').value='ALL';el('usage-scope').textContent=context?`${context.default_workspace.name} · 本设备工具的历史用量`:'当前空间 · 本设备工具的历史用量';if(visible())void refresh();}
  }};
})();
