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
// Exercise the native mint response parser, base-URL normalizer and per-session
// credential cache too: a custom base path must survive all three before send.
const baseFunctions=between('function getEngineMaasBaseUrl(){','function getAccessKeyMintUrl(){');
const mintBody=between('function buildDesktopSessionMintRequestBody(','const MAAS_MINT_PATH=');
const mintSource=between('async function mintAccessKey(','async function exchangeAccessKey(');
const mintNative=new Function('getAccessKeyMintUrl','getQwenWorkClientHeaders','resolveDevAccessToken','getAuthManagerV2','NETWORK_RUNTIME_NO_REPLAY',`
  let serverProvidedBaseUrl,lastMintFailure;
  const DEFAULT_MAAS_BASE_URL='https://forbidden.invalid',logger$fs={info(){},warn(){}},envValue=()=>undefined;
  ${baseFunctions} ${mintBody} ${mintSource}
  return mintAccessKey;
`)(()=>({url:base+'/api/desktop-sessions',source:'fixture'}),()=>({}),()=>undefined,
  ()=>({ensureFreshAccessToken:async()=>'official-login-secret'}),Symbol());
const Broker=new Function(`
  class RuntimeCredentialTransportError extends Error {}
  ${between('const DEFAULT_SESSION_SCOPE$1=','const logger$fs=')}
  return EngineMaasSessionCredentialBroker;
`)();
const broker=new Broker({mint:mintNative,exchange:async()=>({status:'unavailable'}),store:{load:()=>[],save(){},clear(){}}});
const appendSource=between('function appendPath(','function parseJsonObject$2(');
const nativeBuildUrl=new Function(`${appendSource};return n=>appendPath(n,'/chat/completions')`)();
const connect=net.Socket.prototype.connect;
net.Socket.prototype.connect=function(...args){
  let options=args[0];if(Array.isArray(options))options=options[0];
  const host=typeof options==='object'?options.host:args[1];
  if(host!=='127.0.0.1'&&host!=='localhost')throw new Error('Remote networking is forbidden in the desktop probe');
  return connect.apply(this,args);
};
async function run(session){
const minted=await broker.resolve(session,'fixture-identity');
assert.equal(minted.accessKey,capability);
assert.equal(minted.baseUrl,`${base}/sessions/${Buffer.from(session).toString('base64url')}/v1`);
await Promise.all(['prompt-suggestion','session-title','summary','auto-review','subagent'].map(async purpose=>{
  const response=await client.send({
    queryId:'fixture',turns:[],credential:{baseUrl:minted.baseUrl,token:minted.accessKey},
    route:{modelKey:'lite',modelId:'lite',apiProtocol:'openai'},
    input:{purpose,level:'lite',sessionId:session,billingTurnId:'fixture-turn',signal:AbortSignal.timeout(10000)},
    strategy:{headers:{},buildUrl:nativeBuildUrl,buildRequest:()=>({
      model:'lite',messages:[{role:'user',content:`${session}:${purpose}`}],stream:false
    })}
  });
  assert.equal(response.status,200);
  const body=await response.json();assert.equal(body.choices[0].message.content,'OK');
}));
}
await Promise.all(['fixture-session','second-session'].map(run));
await run('fixture-session');
console.log('PASS: installed desktop request code -> scoped local mint -> native relay -> fixture upstream (5 concurrent purposes, distinct and resumed sessions).');
