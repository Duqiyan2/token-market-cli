/* A request snapshot from the scoped log response; opening it never fetches data. */
(() => {
  const dialog=document.getElementById('usage-request-detail');
  if(!dialog)return;
  const body=document.getElementById('usage-detail-body'),closeButton=document.getElementById('usage-detail-close');
  let selected=null,returnFocus=null,backdropPressed=false;
  const node=(tag,value,className)=>{const n=document.createElement(tag);if(value!=null)n.textContent=value;if(className)n.className=className;return n;};
  const number=value=>Number(value||0).toLocaleString('en-US');
  const duration=value=>value==null||value<=0?'—':value>=1000?`${(value/1000).toFixed(2)}s`:`${Math.round(value)}ms`;
  function money(value){let n;try{n=BigInt(value||'0');}catch(_){return '—';}const absolute=n<0n?-n:n;return `${n<0n?'-':''}${(absolute/1000000n).toLocaleString('en-US')}.${(absolute%1000000n).toString().padStart(6,'0')}`;}
  function stamp(value){return new Intl.DateTimeFormat('sv-SE',{timeZone:'Asia/Shanghai',year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',second:'2-digit',hourCycle:'h23'}).format(new Date(value)).replaceAll('-','/');}
  function section(title,rows,note){
    const section=node('section',null,'usage-detail-section'),heading=node('h3',title),list=node('dl');section.append(heading,list);
    for(const {key,label,value,mono,strong,title} of rows){
      const row=node('div',null,strong?'usage-detail-total':''),term=node('dt',label),definition=node('dd',value);
      definition.dataset.detailField=key;
      if(mono)definition.className='usage-detail-mono';
      if(title){term.title=title;definition.title=title;}
      if(key==='requestId'){definition.tabIndex=0;definition.setAttribute('aria-label',`请求 ID：${value}`);}
      row.append(term,definition);list.append(row);
    }
    if(note)section.append(node('p',note,'usage-detail-note'));
    return section;
  }
  function render(){
    const {item,info}=selected;
    const cancelled=item.status?.startsWith('CLIENT_CANCELLED'),success=item.status==='SUCCESS';
    const status=success?'成功':cancelled?'已取消':item.status?'失败':'未记录';
    const generationMs=Number(item.durationMs)-Number(item.ttftMs);
    const speed=success&&item.outputTokens>=100&&item.ttftMs>0&&generationMs>=100?`≈ ${(item.outputTokens/generationMs*1000).toFixed(1)} tok/s`:'—';
    const basic=[
      {key:'time',label:'时间',value:stamp(item.startedAt)},
      {key:'tool',label:'工具',value:info.toolName},
      {key:'workspace',label:'空间',value:info.workspace},
      {key:'model',label:'模型',value:item.model||'—',mono:true},
      {key:'status',label:'状态',value:status},
      {key:'statusCode',label:'状态码',value:item.statusCode>0?String(item.statusCode):'—'},
      {key:'requestId',label:'请求 ID',value:item.requestId||'—',mono:true},
    ];
    if(item.keyName||item.keyId)basic.push({key:'keyName',label:'API Key',value:item.keyName||item.keyId,mono:true});
    body.replaceChildren(
      section('基本信息',basic),
      section('Tokens 用量',[
        {key:'inputTokens',label:'输入',value:number(item.inputTokens),title:'平台记录的输入 Token。'},
        {key:'outputTokens',label:'输出',value:number(item.outputTokens)},
        {key:'cacheReadTokens',label:'缓存读取',value:number(item.cacheReadTokens)},
        {key:'cacheWriteTokens',label:'缓存写入',value:number(item.cacheWriteTokens)},
        {key:'totalTokens',label:'总 Tokens',value:number(item.totalTokens),strong:true,title:'平台记录的总 Token，包含缓存 Token，不重复累加缓存。'},
      ]),
      section('费用',[
        {key:'cost',label:'合计',value:money(item.costMicro),strong:true},
      ],'费用单位与平台流水一致。'),
      section('性能',[
        {key:'speed',label:'输出速度（估算）',value:speed,title:'输出 Token ÷（总耗时 − 首字延迟）。仅成功、输出至少 100 Token、首字延迟有效且生成时间至少 100ms 的记录计算。'},
        {key:'ttft',label:'首字延迟',value:duration(item.ttftMs)},
        {key:'duration',label:'总耗时',value:duration(item.durationMs)},
      ])
    );
    body.querySelector('[data-detail-field="status"]').className=`usage-detail-state ${success?'success':cancelled||!item.status?'cancelled':'failed'}`;
  }
  function close(restoreFocus=true){
    const requestId=selected?.item.requestId,trigger=returnFocus;
    selected=null;returnFocus=null;backdropPressed=false;body.replaceChildren();
    if(dialog.open)dialog.close();
    if(restoreFocus&&window.TokenMarketShell?.page==='usage'){
      const row=[...document.querySelectorAll('#usage-rows [data-request-id]')].find(row=>row.dataset.requestId===requestId);
      const next=row?.querySelector('button')||(trigger?.isConnected?trigger:document.querySelector('#usage-log-view .usage-table-scroll'));
      next?.focus({preventScroll:true});
    }
  }
  function open(item,info,trigger){
    selected={item:{...item},info:{...info}};returnFocus=trigger;render();body.scrollTop=0;
    if(!dialog.open)dialog.showModal();closeButton.focus({preventScroll:true});
  }
  function update(items,info){
    if(!selected)return;
    const item=items.find(item=>item.requestId===selected.item.requestId);
    if(!item)return;
    const next={item:{...item},info:{toolName:info.tools.find(t=>t.id===item.tool)?.name||item.tool||'未知工具',workspace:info.workspace}};
    if(JSON.stringify(next)!==JSON.stringify(selected)){const focused=body.contains(document.activeElement)?document.activeElement.dataset.detailField:null;selected=next;const scroll=body.scrollTop;render();if(focused)[...body.querySelectorAll('[data-detail-field]')].find(n=>n.dataset.detailField===focused)?.focus({preventScroll:true});body.scrollTop=scroll;}
  }
  const outside=event=>{const r=dialog.getBoundingClientRect();return event.clientX<r.left||event.clientX>r.right||event.clientY<r.top||event.clientY>r.bottom;};
  closeButton.onclick=()=>close();
  dialog.addEventListener('keydown',event=>{
    if(event.key!=='Tab')return;
    const focusable=[...dialog.querySelectorAll('button:not(:disabled),[tabindex="0"]')],first=focusable[0],last=focusable.at(-1);
    if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}
    else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}
  });
  dialog.addEventListener('cancel',event=>{event.preventDefault();close();});
  dialog.addEventListener('pointerdown',event=>{backdropPressed=event.target===dialog&&outside(event);});
  dialog.addEventListener('click',event=>{if(backdropPressed&&event.target===dialog&&outside(event))close();backdropPressed=false;});
  dialog.addEventListener('close',()=>{if(!dialog.open){selected=null;returnFocus=null;body.replaceChildren();}});
  window.addEventListener('tokenmarket-page-changed',event=>{if(event.detail.to!=='usage')close(false);});
  window.TokenMarketUsageDetail={open,close,update};
})();
