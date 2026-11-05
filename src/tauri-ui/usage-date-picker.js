/* Date inputs and calendar use Beijing time, independently of the OS timezone. */
(() => {
  const el = id => document.getElementById(id), panel = el('usage-date-panel');
  if (!panel) return;
  const DAY = 86400000, OFFSET = 8 * 3600000;
  const presets = [['today','当天'],['24h','近 24 小时'],['7d','近 7 天'],['14d','近 14 天'],['30d','近 30 天']];
  let selection = {preset:'30d'}, active = 'start', month = '', draftPreset = '30d';
  const stamp = value => new Date(value + OFFSET).toISOString().slice(0,16);
  const dayStart = now => Date.parse(`${stamp(now).slice(0,10)}T00:00:00+08:00`);
  function resolve(value = selection) {
    const now = Date.now();
    if (value.preset === 'custom') return {startTime:value.startTime,endTime:value.liveEnd ? now : value.endTime};
    const startTime = value.preset === '24h' ? now - DAY : dayStart(now) - ((parseInt(value.preset) || 1)-1)*DAY;
    return {startTime,endTime:now};
  }
  function inputStamp(field) { return Date.parse(`${el(`usage-${field}-date`).value}T${el(`usage-${field}-time`).value}:00+08:00`); }
  function fill(field, value) { const s = stamp(value);el(`usage-${field}-date`).value = s.slice(0,10);el(`usage-${field}-time`).value = s.slice(11); }
  function setActive(field) {
    active = field==='end'&&el('usage-live-end').checked?'start':field;
    document.querySelectorAll('[data-date-field]').forEach(n=>n.classList.toggle('active',n.dataset.dateField===active));
    el('usage-calendar-days').setAttribute('aria-label',active==='start'?'选择开始日期':'选择结束日期');
  }
  function renderCalendar() {
    const base = Date.parse(`${month}-01T00:00:00Z`), first = new Date(base).getUTCDay(), start = base - first*DAY;
    const now=Date.now();if(el('usage-live-end').checked)fill('end',now);
    const from = el('usage-start-date').value, to = el('usage-end-date').value;
    el('usage-month-label').textContent = `${month.slice(0,4)}年${Number(month.slice(5))}月`;
    const days = Array.from({length:42},(_,i)=>{
      const d = new Date(start+i*DAY), key = d.toISOString().slice(0,10), button = document.createElement('button');
      button.type='button';button.textContent=d.getUTCDate();button.setAttribute('aria-label',key);
      button.className = `${key.slice(0,7)!==month?'outside ':''}${key>=from&&key<=to?'in-range ':''}${key===from||key===to?'endpoint':''}`;
      button.setAttribute('aria-pressed',String(key===el(`usage-${active}-date`).value));
      button.disabled=key<'1970-01-01' || key>stamp(now).slice(0,10);
      button.onclick=()=>{el(`usage-${active}-date`).value=key;draftPreset='custom';if(active==='start')setActive('end');renderCalendar();renderPresets();const next=el('usage-calendar-days').querySelector(`button[aria-label="${key}"]:not(:disabled)`);(next||el('usage-start-date')).focus();};return button;
    });
    el('usage-calendar-days').replaceChildren(...days);
  }
  function renderPresets() {
    el('usage-presets').replaceChildren(...presets.map(([key,label])=>{
      const button=document.createElement('button');button.type='button';button.textContent=label;button.dataset.rangePreset=key;button.setAttribute('aria-pressed',String(key===draftPreset));
      button.onclick=()=>apply({preset:key});return button;
    }));
  }
  function close(restoreFocus = false) { panel.hidden=true;el('usage-range-trigger').setAttribute('aria-expanded','false');if(restoreFocus)el('usage-range-trigger').focus(); }
  function apply(next) {
    selection=next;
    el('usage-range-label').textContent=presets.find(([key])=>key===next.preset)?.[1] || `${stamp(next.startTime).slice(5,10).replace('-','/')} – ${next.liveEnd?'现在':stamp(next.endTime).slice(5,10).replace('-','/')}`;
    const r=resolve();el('usage-range-trigger').title=`${stamp(r.startTime).replace('T',' ')} — ${stamp(r.endTime).replace('T',' ')}（北京时间）`;
    close(true);window.dispatchEvent(new Event('usage-range-changed'));
  }
  function open() {
    const r=resolve();draftPreset=selection.preset;fill('start',r.startTime);fill('end',r.endTime);
    el('usage-live-end').checked=Boolean(selection.liveEnd);setLive();setActive('start');month=el('usage-start-date').value.slice(0,7);
    el('usage-date-error').hidden=true;renderPresets();renderCalendar();panel.hidden=false;el('usage-range-trigger').setAttribute('aria-expanded','true');
    el('usage-presets').querySelector('[aria-pressed="true"]')?.focus();
  }
  function setLive() {
    const live=el('usage-live-end').checked;
    ['usage-end-date','usage-end-time'].forEach(id=>el(id).disabled=live);
    document.querySelector('[data-date-field="end"]').setAttribute('aria-disabled',String(live));
    if(live)fill('end',Date.now());setActive(active);
  }
  el('usage-range-trigger').onclick=()=>panel.hidden?open():close(true);
  el('usage-date-cancel').onclick=()=>close(true);
  el('usage-date-apply').onclick=()=>{
    const startTime=inputStamp('start'), endTime=el('usage-live-end').checked?Date.now():inputStamp('end');
    let error='';
    if(!Number.isFinite(startTime)||!Number.isFinite(endTime)||startTime<0)error='请输入有效的日期与时间。';
    else if(startTime>endTime)error='结束时间不能早于开始时间。';
    else if(endTime>Date.now())error='结束时间不能晚于当前时刻。';
    el('usage-date-error').textContent=error;el('usage-date-error').hidden=!error;
    if(!error)apply({preset:'custom',startTime,endTime,liveEnd:el('usage-live-end').checked});
  };
  for(const field of ['start','end']) {
    document.querySelector(`[data-date-field="${field}"]`).onclick=()=>{setActive(field);renderCalendar();};
    for(const part of ['date','time'])el(`usage-${field}-${part}`).oninput=()=>{setActive(field);draftPreset='custom';if(part==='date'&&el(`usage-${field}-date`).value)month=el(`usage-${field}-date`).value.slice(0,7);renderPresets();renderCalendar();};
  }
  el('usage-live-end').onchange=()=>{setLive();draftPreset='custom';renderPresets();renderCalendar();};
  for(const [id,delta] of [['usage-month-prev',-1],['usage-month-next',1]])el(id).onclick=()=>{
    const d=new Date(`${month}-01T00:00:00Z`);d.setUTCMonth(d.getUTCMonth()+delta);const y=d.getUTCFullYear();if(y<1970||y>9999)return;month=d.toISOString().slice(0,7);renderCalendar();
  };
  document.addEventListener('pointerdown',event=>{if(!panel.hidden&&!el('usage-range-trigger').parentElement.contains(event.target))close();});
  document.addEventListener('keydown',event=>{
    if(panel.hidden)return;
    if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close(true);}
    if(event.key==='Tab') {
      const focusable=[...panel.querySelectorAll('button:not(:disabled),input:not(:disabled)')],first=focusable[0],last=focusable.at(-1);
      if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}
    }
  });
  window.addEventListener('tokenmarket-page-changed',()=>close());
  window.addEventListener('current-workspace-changed',()=>close());
  window.TokenMarketUsageRange={read:()=>resolve(),key:()=>JSON.stringify(selection),close};
})();
