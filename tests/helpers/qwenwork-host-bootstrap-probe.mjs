import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import {Worker} from 'node:worker_threads';

const root=fs.mkdtempSync(path.join(os.tmpdir(),'tokenmarket-bootstrap-'));
let ready=false,fail=false,spawns=0;
const route={fingerprint:'0'.repeat(64),default_model:'fixture-model',capability:'fixture-capability-abcdefghijklmnopqrstuvwxyz1234567890'};
const server=http.createServer((req,res)=>{
  assert.equal(req.url,`/tokenmarket/${route.capability}/health`);
  res.writeHead(ready?200:503,{'Content-Type':'application/json'});
  res.end(JSON.stringify({contract:'tokenmarket-desktop-v1',fingerprint:route.fingerprint}));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
route.port=server.address().port;
const config=path.join(root,'route.json');fs.writeFileSync(config,JSON.stringify(route));
const worker=path.join(root,'worker.mjs');
fs.writeFileSync(worker,`
import {parentPort} from 'node:worker_threads';
import {createRequire,syncBuiltinESMExports} from 'node:module';
import {EventEmitter} from 'node:events';
const cp=createRequire(import.meta.url)('node:child_process');
cp.spawn=(file,args,options)=>{
  parentPort.postMessage({event:'spawn',file,args,detached:options.detached,stdio:options.stdio});
  const child=new EventEmitter();child.unref=()=>{};
  if(process.env.FIXTURE_FAIL==='1')queueMicrotask(()=>child.emit('error',new Error('fixture failure')));
  if(process.env.FIXTURE_FAIL==='exit')queueMicrotask(()=>child.emit('exit',1));
  return child;
};
syncBuiltinESMExports();
${fs.readFileSync(process.argv[2],'utf8')}
parentPort.postMessage({event:'initialized',route:globalThis.__tmQwenRoute,auxiliaryReady:globalThis.__tmQwenDesktopRelayReady});
`);
async function boot(expectFailure=false,failureMode='1'){
  return new Promise((resolve,reject)=>{
    const child=new Worker(worker,{env:{...process.env,TOKENMARKET_QWENWORK_ROUTE:config,FIXTURE_FAIL:expectFailure?failureMode:'0'}});
    let initialized=false;
    child.on('message',message=>{
      try{
        if(message.event==='spawn'){
          spawns++;assert.deepEqual(message.args,['__qwenwork-host-relay',config,String(process.pid)]);
          assert.equal(message.detached,true);assert.equal(message.stdio,'ignore');
          if(!fail)ready=true;
        }else{
          assert.deepEqual(message.route,route);initialized=true;
          if(expectFailure)assert.equal(message.auxiliaryReady,false,'auxiliary failure must keep primary local routing active');
        }
      }catch(e){reject(e);child.terminate();}
    });
    child.on('error',reject);
    child.on('exit',code=>{if(code===0&&initialized)resolve();else reject(new Error('bootstrap did not finish'));});
  });
}
try{
  await boot();assert.equal(spawns,1);
  await boot();assert.equal(spawns,1,'additional workers must reuse the shared relay');
  ready=false;fail=true;
  await boot(true);
  await boot(true,'exit');
  console.log('PASS: auxiliary relay starts/reuses; its startup failure preserves primary local routing.');
}finally{
  server.closeAllConnections();server.close();fs.rmSync(root,{recursive:true,force:true});
}
