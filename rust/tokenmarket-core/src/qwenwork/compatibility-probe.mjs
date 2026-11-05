// Runs in the installed application's Node runtime with dummy credentials only.
import assert from 'node:assert/strict';
import http from 'node:http';
import net from 'node:net';
import {EventEmitter} from 'node:events';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';

const require=createRequire(import.meta.url);
const capability='fixture-local-capability-abcdefghijklmnopqrstuvwxyz1234567890';
const fingerprint='0'.repeat(64);
const marker=`tokenmarket-local-v2:${fingerprint}`;
let requests=0,spawned=0;
const server=http.createServer(async(req,res)=>{
  try {
    assert.equal(req.url,'/v1/chat/completions');
    assert.equal(req.headers.authorization,`Bearer ${capability}`);
    let bytes='';for await(const chunk of req)bytes+=chunk;
    const body=JSON.parse(bytes);
    assert.equal(body.model,'fixture-model');
    assert.equal(body.custom_model,undefined);
    assert.equal(body.api_key,undefined);
    const round=requests++;
    if(round===1)assert.equal(body.messages.at(-1).tool_call_id,'call_fixture');
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
  const sdk=await import(pathToFileURL(process.argv[2]));
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
  assert.equal(requests,2);assert.equal(spawned,1);
  process.stdout.write('TOKENMARKET_COMPAT_OK\n');
} finally {
  for(const promise of globalThis.__tmLocalRelays?.values()??[])try{(await promise).child.kill();}catch{}
  server.closeAllConnections();server.close();
}
// Older SDK telemetry timers may stay referenced after successful inference.
// This disposable process has completed all checks and owns no user session.
process.exit(0);
