// Each SDK worker ensures the shared desktop relay exists before executing any
// SDK code. No inspector, app archive changes, or platform credentials in JS.
await (async()=>{
  const path=process.env.TOKENMARKET_QWENWORK_ROUTE;
  if(!path)return; // Offline contract probes have no user connection.
  const fs=await import('node:fs'),http=await import('node:http');
  const route=JSON.parse(fs.readFileSync(path,'utf8'));
  if(!/^[a-f0-9]{64}$/.test(route.fingerprint)||!/^[-_a-zA-Z0-9]{40,128}$/.test(route.capability)
    ||!Number.isInteger(route.port)||route.port<1||route.port>65535||!route.default_model)
    throw new Error('Token Market desktop routing is invalid; reconnect the tool');
  const base=`http://127.0.0.1:${route.port}/tokenmarket/${route.capability}`;
  // Main inference uses its own native relay. A desktop-auxiliary startup
  // failure must not terminate the SDK or restore its official model route.
  globalThis.__tmQwenRoute=route;
  globalThis.__tmQwenDesktopRelayReady=false;
  const healthy=()=>new Promise(resolve=>{
    const req=http.get(`${base}/health`,{timeout:500},res=>{
      let body='';res.on('data',chunk=>{body+=chunk;if(body.length>1024)req.destroy();});
      res.on('end',()=>{try{const data=JSON.parse(body);resolve(res.statusCode===200&&data.fingerprint===route.fingerprint&&data.contract==='tokenmarket-desktop-v1');}catch{resolve(false);}});
      res.on('error',()=>resolve(false));
    });req.on('error',()=>resolve(false));req.on('timeout',()=>{req.destroy();resolve(false);});
  });
  try {
   if(!await healthy()){
    const {spawn}=await import('node:child_process');
    const child=spawn(__BRIDGE_BINARY__,['__qwenwork-host-relay',path,String(process.pid)],{
      stdio:'ignore',windowsHide:true,detached:true,
      env:{...process.env,TOKEN_MARKET_GATEWAY_API_BASE_URL:__GATEWAY__,TOKEN_MARKET_CONSOLE_API_BASE_URL:__CONSOLE__}
    });
    let failed=false;
    child.once('error',()=>{failed=true;});child.once('exit',()=>{failed=true;});child.unref();
    const deadline=Date.now()+15000;
    while(!await healthy()){
      if(failed||Date.now()>deadline)throw new Error('Token Market desktop relay is unavailable; reconnect the tool');
      await new Promise(resolve=>setTimeout(resolve,50));
    }
   }
   globalThis.__tmQwenDesktopRelayReady=true;
  } catch {
    try{fs.writeSync(2,'[TokenMarketLocal] Desktop auxiliary relay unavailable; primary inference remains on the local route.\n');}catch{}
  }
})();
