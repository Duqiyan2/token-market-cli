/* TOKENMARKET_LOCAL_RELAY_V2 — never receives the platform API key. */
if(t.custom_model?.url==="https://tokenmarket.local.invalid/v1/chat/completions"){
  c();
  const __tm=t.custom_model,__marker=__tm.parameters?.api_key;
  if(typeof __marker!=="string"||!/^tokenmarket-local-v2:[a-f0-9]{64}$/.test(__marker))
    throw new Error("Token Market local credential marker is invalid; reconnect the tool");
  const __fingerprint=__marker.slice("tokenmarket-local-v2:".length);
  globalThis.__tmLocalRelays??=new Map();
  let __relay=globalThis.__tmLocalRelays.get(__fingerprint);
  if(!__relay){
    __relay=new Promise((resolve,reject)=>{
      const child=require("node:child_process").spawn(__BRIDGE_BINARY__,["__qwenwork-relay",__fingerprint],{
        stdio:["pipe","pipe","ignore"],windowsHide:true,
        env:{...process.env,TOKEN_MARKET_GATEWAY_API_BASE_URL:__GATEWAY__,TOKEN_MARKET_CONSOLE_API_BASE_URL:__CONSOLE__}
      });
      let buffer="",settled=false;
      const fail=()=>{if(!settled){settled=true;clearTimeout(timer);child.kill();reject(new Error("Token Market local relay is unavailable; reconnect the tool"))}};
      const timer=setTimeout(fail,15000);
      child.once("error",fail);child.once("exit",()=>{globalThis.__tmLocalRelays.delete(__fingerprint);fail()});
      child.stdout.on("data",chunk=>{
        if(settled)return;
        buffer+=chunk.toString("utf8");if(buffer.length>4096)return fail();
        if(!buffer.includes("\n"))return;
        try{
          const value=JSON.parse(buffer.slice(0,buffer.indexOf("\n"))),url=new URL(value.baseUrl);
          if(url.protocol!=="http:"||url.hostname!=="127.0.0.1"||url.pathname!=="/v1"||!/^[-_a-zA-Z0-9]{40,128}$/.test(value.token))return fail();
          settled=true;clearTimeout(timer);resolve({baseUrl:value.baseUrl,token:value.token,child});
        }catch{return fail()}
      });
      process.once("exit",()=>child.kill());
    });
    globalThis.__tmLocalRelays.set(__fingerprint,__relay);
  }
  const __local=await __relay;
  if(__local.child.exitCode!==null||__local.child.killed)throw new Error("Token Market local relay has stopped");
  c();
  const __model=__tm.model||s;
  const __provider={providerId:"tokenmarket-local",adapter:"openai-compatible",authType:"bearer",
    baseUrl:__local.baseUrl,apiKey:__local.token,
    model:{key:__model,modelId:__model,displayName:__model,maxTokensField:"max_tokens",
      capabilities:{tools:!0,vision:__tm.isVl!==!1,thinking:__EMPTY_THINKING__()}},timeouts:{firstPayloadTimeoutMs:60000}};
  try{require("node:fs").writeSync(2,`[TokenMarketLocal] model=${__model} relay=loopback\n`)}catch{}
  const __request=__BUILD_REQUEST__(t,{...g,target:{kind:"direct",provider:__provider},beforeSend:c,sessionForm:A});
  c();try{return __OPEN_STREAM__(__request,n)}finally{l?.()}
}
