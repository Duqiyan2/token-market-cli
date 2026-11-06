// Runs in the installed application's Node runtime with dummy credentials only.
import assert from 'node:assert/strict';
import http from 'node:http';
import net from 'node:net';
import fs from 'node:fs';
import {EventEmitter} from 'node:events';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';

const require=createRequire(import.meta.url);
const capability='fixture-local-capability-abcdefghijklmnopqrstuvwxyz1234567890';
const fingerprint='0'.repeat(64);
const marker=`tokenmarket-local-v2:${fingerprint}`;
let requests=0,spawned=0,authRequests=0;
const server=http.createServer(async(req,res)=>{
  try {
    if(req.url==='/api/v1/userinfo') {
      assert.equal(req.method,'GET');assert.equal(req.headers.authorization,'Bearer fixture-old-host-token');authRequests++;
      res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({id:'fixture-legacy-user',name:'Fixture'}));return;
    }
    assert.equal(req.url,'/v1/chat/completions');
    assert.equal(req.headers.authorization,`Bearer ${capability}`);
    let bytes='';for await(const chunk of req)bytes+=chunk;
    const body=JSON.parse(bytes);
    assert.equal(body.model,'fixture-model');
    assert.equal(body.custom_model,undefined);
    assert.equal(body.api_key,undefined);
    const round=requests++;
    assert.equal(req.headers['session-id'],round===3?'legacy-task':'fixture');
    if(round===1)assert.equal(body.messages.at(-1).tool_call_id,'call_fixture');
    if(round===3)assert.deepEqual(body.messages,[{role:'user',content:'Old question'},{role:'assistant',content:'Old answer'},{role:'user',content:'Continue the old task'}]);
    const delta=round===0?{tool_calls:[{index:0,id:'call_fixture',type:'function',function:{name:'fixture_time',arguments:'{}'}}]}:{content:'OK'};
    const event=(delta,finish_reason)=>JSON.stringify({id:'fixture',object:'chat.completion.chunk',created:1,model:'fixture-model',choices:[{index:0,delta,finish_reason}]});
    res.writeHead(200,{'Content-Type':'text/event-stream'});
    res.end(`data: ${event(delta,null)}\n\ndata: ${event({},round===0?'tool_calls':'stop')}\n\ndata: [DONE]\n\n`);
  }catch {res.writeHead(400);res.end('Fixture mismatch');}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));

// Fail closed if any SDK initializer or fallback attempts remote networking.
const connect=net.Socket.prototype.connect;
net.Socket.prototype.connect=function(...args){
  let options=args[0];
  if(Array.isArray(options))options=options[0];
  const host=typeof options==='object'?options.host:(typeof args[1]==='string'?args[1]:'localhost');
  if(host!=='127.0.0.1'&&host!=='localhost')throw new Error('Non-loopback networking is forbidden in the compatibility probe');
  return connect.apply(this,args);
};
require('node:child_process').spawn=(_file,args)=>{
  assert.deepEqual(args,['__qwenwork-relay',fingerprint]);spawned++;
  const child=new EventEmitter();child.stdout=new EventEmitter();child.stdin=new EventEmitter();
  child.exitCode=null;child.killed=false;
  child.kill=()=>{if(!child.killed){child.killed=true;child.exitCode=0;child.emit('exit',0);}};
  queueMicrotask(()=>child.stdout.emit('data',Buffer.from(JSON.stringify({baseUrl:`http://127.0.0.1:${server.address().port}/v1`,token:capability})+'\n')));
  return child;
};
try {
  process.env.QODERCN_SERVER_ENDPOINT='https://tokenmarket.local.invalid';
  // Run the installed SDK's actual host-token user-info/request methods. Only
  // transport and small parsing dependencies are supplied by this local fixture.
  const source=fs.readFileSync(process.argv[2],'utf8');
  function method(start,end){const i=source.indexOf(start);assert(i>=0,start);const j=source.indexOf(end,i);assert(j>i,end);return source.slice(i,j);}
  const requestMethod=method('async openApiJsonRequest(A){','async exchangePersonalToken(');
  const userMethod=method('async fetchOpenApiUserInfo(A,e,t={}){','async fetchOrganizationTags(');
  const hostTokenMethods=method('async initFromHostJobToken(A,e={}){','async initReadOnlyFromStore(');
  const externalTokenMethod=method('async applyExternalToken(A,e,t={}){','async applyServiceAccountToken(');
  const binding=(text,pattern)=>{const found=text.match(pattern);assert(found,pattern.source);return found[1];};
  const dependencies=new Map([
    [binding(requestMethod,/let e,t=([\w$]+)\("openapi"\)/),purpose=>sdk.resolveEndpoint(purpose).url],
    [binding(requestMethod,/qoder\/\$\{await ([\w$]+)\(\)\}/),async()=>'fixture'],
    [binding(requestMethod,/let n=await ([\w$]+)\(\{operation:A.operation/),async request=>{
      assert.equal(request.operation,'auth.getUserInfo');assert.equal(request.endpointType,'openapi');
      assert.equal(new URL(request.url).pathname,'/api/v1/userinfo');
      assert.notEqual(new URL(request.url).hostname,'tokenmarket.local.invalid');
      return fetch(`http://127.0.0.1:${server.address().port}${request.path}`,request.init);
    }],
    [binding(requestMethod,/throw new ([\w$]+)\(t,n.status\)/),Error],
    [binding(userMethod,/r=([\w$]+)\(n\)\?n:null/),value=>value!==null&&typeof value==='object'],
    [binding(userMethod,/a=([\w$]+)\(r,\["id"/),(value,keys)=>keys.map(key=>value?.[key]).find(v=>typeof v==='string')],
    [binding(userMethod,/is_data_policy_modifiable:([\w$]+)\(r,/),(value,keys)=>keys.map(key=>value?.[key]).find(v=>typeof v==='boolean')],
    [binding(externalTokenMethod,/([\w$]+)\.debug\(`\[auth\] External token applied/),{debug(){}}],
  ]);
  const AuthClient=new Function(...dependencies.keys(),`return class {
    constructor(){this.authEpoch=0;this.dataPolicyPolling={start(){},stop(){}};this.enterprisePluginsPolling={start(){},stop(){}};}
    getBearerToken(token){return token;} stopRefreshTimer(){} resetServiceAccountAuthState(){}
    regenerateRuntimeFields(){} async createWasmContext(){} refreshOrganizationTagsInBackground(){}
    updateHostJobTokenExpiry(){} clearHostJobTokenRefreshState(){} notifyAuthStateChanged(){}
    syncEndpointAfterLogin(){if(${binding(requestMethod,/let e,t=([\w$]+)\("openapi"\)/)}('center').includes('tokenmarket.local.invalid'))throw Error('Invalid control origin');}
    ${requestMethod} ${userMethod} ${hostTokenMethods} ${externalTokenMethod}
  }`)(...dependencies.values());
  const sdk=await import(pathToFileURL(process.argv[2]));
  assert.equal(sdk.resolveEndpoint('openapi').url,'https://tokenmarket.local.invalid','inactive routes must preserve the SDK endpoint contract');
  const policy=sdk.parseModelPolicy({model:'fixture-model',custom_model:{provider:'openai',style:'openai',url:'https://tokenmarket.local.invalid/v1/chat/completions',api_key:marker}});
  assert.equal(policy.customModel.api_key,marker);
  assert.throws(()=>sdk.assembleModel('fixture-model',{getModel:()=>undefined},{...policy.customModel,api_key:'invalid-marker'}),/reconnected/);
  const identity='fixture-identity';
  const dispatch=sdk.requestDispatcher({getModelRequestIdentity:()=>identity});
  async function turn(messages){
    const model=sdk.assembleModel('fixture-model',{getModel:()=>undefined},policy.customModel);
    assert.equal(model.customModel.url,policy.customModel.url);
    assert.equal(model.customModel.parameters.api_key,marker);
    const stream=await dispatch({route:{kind:'qoder',identity,source:'system'},model:'fixture-model'},{
      model_config:model.modelConfig,custom_model:model.customModel,messages,
      tools:[{type:'function',function:{name:'fixture_time',parameters:{type:'object',properties:{}}}}],
      parameters:{max_tokens:64},request_id:'fixture',session_id:'fixture'
    },{signal:AbortSignal.timeout(10000),reason:'fixture'},1);
    const events=[];for await(const event of stream)events.push(event);return events;
  }
  const messages=[{role:'user',content:'Read the fixture time.'}];
  const first=await turn(messages);assert(first.some(e=>e.toolCall?.name==='fixture_time'));
  const second=await turn([...messages,{role:'assistant',content:null,tool_calls:[{id:'call_fixture',type:'function',function:{name:'fixture_time',arguments:'{}'}}]},{role:'tool',tool_call_id:'call_fixture',content:'fixture time'}]);
  assert.equal(second.map(e=>e.content??'').join(''),'OK');
  // Reproduce a system-model call (review/subagent) without any BYOK policy.
  // The original implementation fell through to the forbidden cloud route.
  globalThis.__tmQwenRoute={fingerprint,default_model:'fixture-model'};
  for(const purpose of ['openapi','center']) {
    const endpoint=sdk.resolveEndpoint(purpose);
    assert.notEqual(new URL(endpoint.url).hostname,'tokenmarket.local.invalid',`old-task ${purpose} authentication must not resolve the inference blocking marker`);
    assert.equal(new URL(endpoint.url).protocol,'https:');
  }
  for(const purpose of ['inference','securityInference','webSearch'])
    assert.equal(sdk.resolveEndpoint(purpose).url,'https://tokenmarket.local.invalid','model and search fallbacks remain blocked');
  process.env.QODERCN_SERVER_ENDPOINT='https://control.fixture.invalid';
  assert.equal(sdk.resolveEndpoint('openapi').url,'https://control.fixture.invalid','explicit user endpoints must be preserved');
  process.env.QODERCN_SERVER_ENDPOINT='https://tokenmarket.local.invalid';
  const oldTaskAuth=new AuthClient();await oldTaskAuth.initFromHostJobToken({token:'fixture-old-host-token'});
  assert.equal(oldTaskAuth.cachedUserInfo.uid,'fixture-legacy-user');
  assert.equal(oldTaskAuth.cachedUserInfo.login_method,'job_token');
  assert.equal(oldTaskAuth.refreshStrategy,'host-job-token');
  assert.equal(authRequests,1);
  // Desktop auxiliary startup may fail independently. SDK inference must
  // remain alive and use its own native relay, never the official fallback.
  globalThis.__tmQwenDesktopRelayReady=false;
  const auxiliary=await dispatch({route:{kind:'qoder',identity,source:'system'},model:'flash'},
    {messages:[{role:'user',content:'Auxiliary fixture'}],parameters:{max_tokens:32},request_id:'aux',session_id:'fixture'},
    {signal:AbortSignal.timeout(10000),reason:'fixture'},1);
  const events=[];for await(const event of auxiliary)events.push(event);
  assert.equal(events.map(e=>e.content??'').join(''),'OK');
  const resumed=await dispatch({route:{kind:'qoder',identity,source:'system'},model:'legacy-official-model'},
    {messages:[{role:'user',content:'Old question'},{role:'assistant',content:'Old answer'},{role:'user',content:'Continue the old task'}],parameters:{max_tokens:32},request_id:'resume',session_id:'legacy-task'},
    {signal:AbortSignal.timeout(10000),reason:'fixture-resume'},1);
  const resumedEvents=[];for await(const event of resumed)resumedEvents.push(event);
  assert.equal(resumedEvents.map(e=>e.content??'').join(''),'OK');
  assert.equal(requests,4);assert.equal(spawned,1);
  process.stdout.write('TOKENMARKET_COMPAT_OK\n');
} finally {
  for(const promise of globalThis.__tmLocalRelays?.values()??[])try{(await promise).child.kill();}catch{}
  server.closeAllConnections();server.close();
}
// Older SDK telemetry timers may stay referenced after successful inference.
// This disposable process has completed all checks and owns no user session.
process.exit(0);
