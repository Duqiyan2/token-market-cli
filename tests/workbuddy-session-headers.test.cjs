// Read-only installed-runtime contract check. No application or user profile is opened.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const {AsyncLocalStorage}=require('node:async_hooks');
const {randomUUID}=require('node:crypto');
const runtime='/Applications/WorkBuddy.app/Contents/Resources/app.asar.unpacked/node_modules/@genie/agent-harness/cli/dist-server/shared.cjs';

test('installed WorkBuddy derives its conversation header from each request context', {skip:!fs.existsSync(runtime)}, async()=>{
  const source=fs.readFileSync(runtime,'utf8');
  const start=source.indexOf('axiosToFetchAdapter(){return async(ei,ea)=>{');
  assert(start>=0,'WorkBuddy transport contract changed');
  const end=source.indexOf(';let eX=(0,',start);
  // Keep the native header preparation through CONVERSATION_ID_HEADER assignment.
  assert(end>start && end-start<5000,'WorkBuddy conversation header contract changed');
  const prepare=source.slice(start,end)+';return eN}}';
  const constants={};
  for(const name of ['CONVERSATION_ID_HEADER','CONVERSATION_REQUEST_ID_HEADER','CONVERSATION_MESSAGE_ID_HEADER','REQUEST_ID_HEADER']){
    const match=source.match(new RegExp('\\.'+name+'="([^"]+)"'));
    assert(match,name);constants[name]=match[1];
  }
  assert.equal(constants.CONVERSATION_ID_HEADER,'X-Conversation-ID');
  const context=new AsyncLocalStorage();
  const transport=new Function('eH','tW','e3','ez',`return {${prepare}}`)(
    {currentTurnCarrier:()=>({}),getRequestOwner:()=>undefined,currentTurnContext:()=>undefined,
      getEffectiveConversationRequestId:()=>randomUUID(),getEffectiveAgentPurpose:()=>undefined},
    {hd:{getStore:()=>undefined}},{nH:randomUUID},constants);
  transport.sessionManager={getCurrent:()=>context.getStore()};
  const request=transport.axiosToFetchAdapter();
  const sessions=['workbuddy-a','workbuddy-b','workbuddy-a'];
  const headers=await Promise.all(sessions.map(id=>context.run({id},()=>request('http://127.0.0.1/v1/chat/completions'))));
  assert.deepEqual(headers.map(value=>value['X-Conversation-ID']),sessions);
  assert.equal(new Set(headers.map(value=>value['X-Request-ID'])).size,3);
});
