// The real SDK parser, request assembler and SSE decoder talk to the native
// loopback relay. Only the Rust test's trusted upstream ever receives its key.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';

async function main() {
  const sdk=await import(pathToFileURL(process.argv[2]));
  const fingerprint=process.argv[3];
  const marker=`tokenmarket-local-v2:${fingerprint}`;
  const endpoint='https://tokenmarket.local.invalid/v1/chat/completions';
  const model='fixture-model';
  const identity='fixture-identity';
  const dispatch=sdk.requestDispatcher({getModelRequestIdentity:()=>identity});
  const policy=sdk.parseModelPolicy({model,custom_model:{provider:'openai',style:'openai',url:endpoint,api_key:marker}});
  assert.equal(policy.customModel.api_key,marker);
  assert.throws(()=>sdk.assembleModel(model,{getModel:()=>undefined},
    {...policy.customModel,api_key:'fixture-invalid-marker'}), /reconnected/);
  const tools=[{type:'function',function:{name:'fixture_time',parameters:{type:'object',properties:{}}}}];
  async function turn(history) {
    const assembled=sdk.assembleModel(model,{getModel:()=>undefined},policy.customModel);
    assert.equal(assembled.customModel.url,endpoint);
    assert.equal(assembled.customModel.parameters.api_key,marker);
    const stream=await dispatch({route:{kind:'qoder',identity,source:'system'},model}, {
      model_config:assembled.modelConfig,custom_model:assembled.customModel,messages:history,tools,
      parameters:{max_tokens:64},request_id:'fixture-request',session_id:'fixture-session'
    }, {signal:AbortSignal.timeout(10000),reason:'fixture'},1);
    const events=[];for await (const event of stream)events.push(event);
    return events;
  }
  try {
    const messages=[{role:'user',content:'Read the fixture time.'}];
    const first=await turn(messages);
    assert.ok(first.some(e=>e.toolCall?.name==='fixture_time'));
    const second=await turn([...messages,
      {role:'assistant',content:null,tool_calls:[{id:'call_fixture',type:'function',function:{name:'fixture_time',arguments:'{}'}}]},
      {role:'tool',tool_call_id:'call_fixture',content:'fixture result'}]);
    assert.equal(second.map(e=>e.content??'').join(''),'OK');
    console.log('PASS: real SDK -> native local relay -> trusted upstream; streamed tool call and tool result completed.');
  } finally {
    for (const promise of globalThis.__tmLocalRelays?.values()??[]) {
      try { const relay=await promise;relay.child.stdin.end();relay.child.kill(); } catch {}
    }
  }
}
main().catch(e=>{console.error(`${e.name}: ${e.message}`);process.exitCode=1;});
