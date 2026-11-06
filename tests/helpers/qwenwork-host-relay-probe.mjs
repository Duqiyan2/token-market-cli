// Exercise the installed desktop's captured-fetch request method, not a
// reimplementation. All networking is restricted to the local Rust fixtures.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import net from 'node:net';

const [archive,base,capability]=process.argv.slice(2);
process.noAsar=true;
const fd=fs.openSync(archive,'r');
const first=Buffer.alloc(8);fs.readSync(fd,first,0,8,0);
const header=Buffer.alloc(first.readUInt32LE(4));fs.readSync(fd,header,0,header.length,8);
const index=JSON.parse(header.subarray(8,8+header.readUInt32LE(4)).toString());
const entry=index.files.out.files.main.files['main.js'];
assert(!entry.unpacked);
const bytes=Buffer.alloc(entry.size);fs.readSync(fd,bytes,0,bytes.length,8+header.length+Number(entry.offset));fs.closeSync(fd);
const source=bytes.toString();
function between(start,end){
  const i=source.indexOf(start);assert(i>=0,`Desktop contract missing: ${start}`);
  const j=source.indexOf(end,i+start.length);assert(j>i);
  return source.slice(i,j);
}
const headerFunction=between('function buildAuthenticatedUpstreamHeaders(', 'function asRecord$j(');
const send=between('async send(e){const r=e.strategy.buildUrl', 'async executeTools(');
// Keep the same captured fetch behavior as UtilityModelClient's constructor.
const DesktopClient=new Function('logger$dC','modelIdForLog','getDesktopOriginAttributionHeaders',
  'applyRequestOverrides','isMcpDiscoveryPresentationHeader',
  `${headerFunction};return class { constructor(){this.fetchImpl=fetch;} ${send} }`)(
    {info(){}},x=>x,()=>({}),(body)=>body,()=>false);
const client=new DesktopClient();
const connect=net.Socket.prototype.connect;
net.Socket.prototype.connect=function(...args){
  let options=args[0];if(Array.isArray(options))options=options[0];
  const host=typeof options==='object'?options.host:args[1];
  if(host!=='127.0.0.1'&&host!=='localhost')throw new Error('Remote networking is forbidden in the desktop probe');
  return connect.apply(this,args);
};
const minted=await fetch(`${base}/api/desktop-sessions`,{method:'POST',headers:{
  Authorization:'Bearer official-login-secret','Content-Type':'application/json'
},body:JSON.stringify({session_id:'fixture-session'})}).then(r=>r.json());
assert.equal(minted.access_token,capability);
assert.equal(minted.base_url,base);
for(const purpose of ['prompt-suggestion','session-title','summary','auto-review','subagent']){
  const response=await client.send({
    queryId:'fixture',turns:[],credential:{baseUrl:minted.base_url,token:minted.access_token},
    route:{modelKey:'lite',modelId:'lite',apiProtocol:'openai'},
    input:{purpose,level:'lite',sessionId:'fixture-session',billingTurnId:'fixture-turn',signal:AbortSignal.timeout(10000)},
    strategy:{headers:{},buildUrl:url=>`${url}/v1/chat/completions`,buildRequest:()=>({
      model:'lite',messages:[{role:'user',content:purpose}],stream:false
    })}
  });
  assert.equal(response.status,200);
  const body=await response.json();assert.equal(body.choices[0].message.content,'OK');
}
console.log('PASS: installed desktop request code -> local mint -> native relay -> fixture upstream (5 purposes).');
