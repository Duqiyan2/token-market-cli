// Production adapter, fake upstreams and temporary account data only.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const path=require('node:path');
const script=String.raw`
import fs from 'node:fs';import os from 'node:os';import path from 'node:path';import http from 'node:http';import {createHash} from 'node:crypto';import {pathToFileURL} from 'node:url';import assert from 'node:assert/strict';
const root=fs.mkdtempSync(path.join(os.tmpdir(),'tokenmarket-unified-unit-'));const calls=[];let forbidden=0;
const evil=http.createServer((_q,s)=>{forbidden++;s.end('{}')});await new Promise(r=>evil.listen(0,'127.0.0.1',r));
const officialCalls=[];const official=http.createServer((q,s)=>{const chunks=[];q.on('data',b=>chunks.push(b));q.on('end',()=>{officialCalls.push({path:q.url,headers:q.headers,body:JSON.parse(Buffer.concat(chunks))});s.writeHead(200,{'Content-Type':'application/json'});s.end('{}');});});await new Promise(r=>official.listen(0,'127.0.0.1',r));
const upstream=http.createServer((q,s)=>{const chunks=[];q.on('data',b=>chunks.push(b));q.on('end',()=>{const body=JSON.parse(Buffer.concat(chunks));calls.push({path:q.url,headers:q.headers,body});
 if(process.env.SCENARIO==='redirect'){s.writeHead(302,{Location:'http://127.0.0.1:'+evil.address().port+'/stolen'});s.end();return}
 if(process.env.SCENARIO==='empty'){s.writeHead(204);s.end();return}
 if(process.env.SCENARIO==='upstream-error'){s.writeHead(500,{'Content-Type':'application/json'});s.end('{"error":{"message":"fixture"}}');return}
 s.writeHead(200,{'Content-Type':'text/event-stream'});s.end('data: '+JSON.stringify({model:body.model})+'\n\ndata: [DONE]\n\n');});});await new Promise(r=>upstream.listen(0,'127.0.0.1',r));
const provider={baseURL:'http://127.0.0.1:'+upstream.address().port+'/v1',apiKey:'sk-fake-unified',model:'chosen-fixture',contextLimit:65536,outputLimit:1024};
fs.mkdirSync(path.join(root,'desktop-main'),{recursive:true});fs.mkdirSync(path.join(root,'opencode'),{recursive:true});fs.writeFileSync(path.join(root,'desktop-main/dumate-app.json'),JSON.stringify({customLLMProvider:null}));
const fingerprint=createHash('sha256').update(JSON.stringify(['baseURL','apiKey','model','contextLimit','outputLimit'].map(k=>provider[k]))).digest('hex');
fs.writeFileSync(path.join(root,'opencode/tokenmarket-unified.json'),JSON.stringify({unified:{protocol:3,owner:'cn.tokenmarket',provider,baseURL:provider.baseURL,model:provider.model,providerFingerprint:process.env.SCENARIO==='tampered'?'0'.repeat(64):fingerprint}}));
process.env.XDG_CONFIG_HOME=root;process.env.DUMATE_APP_VER='1.0.89.447';process.env.OPENCODE_NEW_RUNTIME=process.env.SCENARIO==='unsupported'?'2':process.env.SCENARIO.startsWith('runtime-1')?'1':'0';process.env.DUMATE_LOGIN_MODE='personal';delete process.env['X-Dumate-Prd-Source'];
const modulePath=path.join(root,'adapter.mjs');fs.copyFileSync(process.env.ADAPTER_SOURCE,modulePath);const adapter=await (await import(pathToFileURL(modulePath))).default();
const config={provider:{QianfanPersonalQuota:{options:{baseURL:'http://127.0.0.1:'+official.address().port+'/v1',apiKey:'official-fixture-secret'},models:{fixture:{limit:{}}}}}};
const invalid=['tampered','unsupported'].includes(process.env.SCENARIO);if(invalid)assert.throws(()=>adapter.config(config));else adapter.config(config);
const payload={model:'official-hardcoded-model',messages:[{role:'user',content:'fixture'}],max_tokens:100000};
const make=(url,body=payload)=>fetch(url,{method:'POST',headers:{Authorization:'Bearer baidu-fake-secret',Cookie:'baidu=fake', 'X-Dumate-Inapp-Key':'fake-inapp'},body:JSON.stringify(body)});
try {
 if(invalid){await assert.rejects(()=>make('https://official.invalid/v1/chat/completions'));assert.equal(calls.length,0);}
 else if(process.env.SCENARIO==='redirect'){await assert.rejects(()=>make('https://official.invalid/v1/chat/completions'));assert.equal(forbidden,0);assert.equal(calls.length,1);}
 else if(process.env.SCENARIO==='empty'){const response=await make('https://official.invalid/v1/chat/completions');assert.equal(response.status,204);assert.equal(await response.text(),'');assert.equal(calls.length,1);}
 else if(process.env.SCENARIO==='upstream-error'){const response=await make('https://official.invalid/v1/chat/completions');assert.equal(response.status,500);assert.match(await response.text(),/fixture/);assert.equal(calls.length,1);}
 else if(process.env.SCENARIO==='already-aborted'){const controller=new AbortController();controller.abort();await assert.rejects(()=>fetch('https://official.invalid/v1/chat/completions',{method:'POST',body:JSON.stringify(payload),signal:controller.signal}));await new Promise(r=>setTimeout(r,30));assert.equal(calls.length,0);}
 else if(process.env.SCENARIO==='session-isolation'){
  const send=async(session,headers={})=>{
   const request=new Request('https://official.invalid/v1/chat/completions',{method:'POST',headers:{'X-Dumate-Session-Id':session,...headers},body:JSON.stringify({...payload,messages:[{role:'user',content:session}]})});
   await(await fetch(request)).text();
  };
  await Promise.all(['chat-a','chat-b','chat-a'].map(id=>send(id)));
  for(const call of calls){assert.equal(call.headers['x-dumate-session-id'],call.body.messages[0].content);assert.equal(call.headers['x-session-id'],undefined);}
  await send('native-session',{'X-Session-Id':'explicit-session'});assert.equal(calls.at(-1).headers['x-session-id'],'explicit-session');assert.equal(calls.at(-1).headers['x-dumate-session-id'],'native-session');
  await send('native-session',{'X-Session-Id':' '});assert.equal(calls.at(-1).headers['x-session-id'],undefined);assert.equal(calls.at(-1).headers['x-dumate-session-id'],'native-session');
  await(await make('https://official.invalid/v1/chat/completions')).text();assert.equal(calls.at(-1).headers['x-session-id'],undefined);
  await send('s'.repeat(257));assert.equal(calls.at(-1).headers['x-dumate-session-id'].length,257);
 }
 else if(['official-media','runtime-1-media'].includes(process.env.SCENARIO)){
  const base='http://127.0.0.1:'+official.address().port;
  for(const endpoint of ['/v1/images/generations','/v1/videos/generations'])await (await make(base+endpoint,{model:'official-media-model',prompt:'fixture'})).text();
  await (await make(base+'/v1/chat/completions',{...payload,model:'model-mm'})).text();
  for(const type of ['image_url','video_url'])await (await make(base+'/v1/chat/completions',{...payload,messages:[{role:'user',content:[{type,[type]:{url:'https://media.invalid/fixture'}}]}]})).text();
  await (await make('https://tokenmarket.dumate.invalid/v1/chat/completions',{...payload,model:'model-mm'})).text();
  for(const endpoint of ['/v1/images/generations','/v1/videos/generations'])await (await fetch(new Request('https://tokenmarket.dumate.invalid'+endpoint,{method:'POST',headers:{Authorization:'Bearer tokenmarket-engine-local','Content-Type':'application/json'},body:JSON.stringify({model:'official-generation',prompt:'fixture'})}))).text();
  const model={providerID:'qianfan-multimodal',limit:{output:32000}};adapter['chat.params']({model});assert.equal(model.limit.output,32000);
  assert.equal(calls.length,0);assert.equal(officialCalls.length,8);assert.equal(officialCalls[0].body.model,'official-media-model');assert.equal(officialCalls[2].body.model,'model-mm');
  for(const call of officialCalls.slice(5))assert.equal(call.headers.authorization,'Bearer official-fixture-secret');assert(officialCalls.every(c=>c.headers.authorization!=='Bearer sk-fake-unified'));
 }
 else {
  const response=await make('https://official.invalid/v1/chat/completions');assert.equal(response.status,200);assert.match(await response.text(),/chosen-fixture/);
  assert.equal(calls.length,1);assert.equal(calls[0].body.model,'chosen-fixture');assert.equal(calls[0].body.max_tokens,1024);assert.equal(calls[0].headers.authorization,'Bearer sk-fake-unified');assert.equal(calls[0].headers.cookie,undefined);assert.equal(calls[0].headers['x-dumate-inapp-key'],undefined);
  provider.model='saved-but-not-active';fs.writeFileSync(path.join(root,'desktop-main/dumate-app.json'),JSON.stringify({customLLMProvider:null}));
  await (await make('https://official.invalid/v1/chat/completions')).text();assert.equal(calls[1].body.model,'chosen-fixture');
  await (await make('https://official.invalid/v1/chat/completions',{...payload,max_completion_tokens:100000})).text();assert.equal(calls[2].body.max_completion_tokens,1024);assert.equal(calls[2].body.max_tokens,undefined);
  await assert.rejects(()=>make('https://official.invalid/v1/messages'));await assert.rejects(()=>make('http://127.0.0.1:1/gateway/v2/agent/ai_assistant/run'));
  assert.equal(calls.length,3);
 }
 console.log(JSON.stringify({scenario:process.env.SCENARIO,upstreamCalls:calls.length,forbiddenDestinationCalls:forbidden}));
} finally {await new Promise(r=>upstream.close(r));await new Promise(r=>evil.close(r));await new Promise(r=>official.close(r));fs.rmSync(root,{recursive:true,force:true});}
`;
for(const scenario of ['forward-and-pin','runtime-1','tampered','unsupported','redirect','empty','upstream-error','already-aborted','session-isolation','official-media','runtime-1-media'])test(`DuMate unified adapter ${scenario}`,()=>{
 const result=spawnSync(process.execPath,['--input-type=module','-e',script],{env:{...process.env,SCENARIO:scenario,ADAPTER_SOURCE:path.resolve(__dirname,'../rust/tokenmarket-core/src/dumate/unified/plugin.js')},encoding:'utf8',timeout:20000});
 assert.equal(result.status,0,result.stderr);const report=JSON.parse(result.stdout.trim());assert.equal(report.forbiddenDestinationCalls,0);
});
