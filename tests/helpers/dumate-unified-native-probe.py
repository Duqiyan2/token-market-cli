#!/usr/bin/env python3
"""Cold-start unified adapter against official isolated DuMate backend + engine.
Fake credentials, localhost upstream, no external network, no real user data.
"""
import argparse,contextlib,hashlib,importlib.util,io,json,os,pathlib,re,socket,sqlite3,subprocess,sys,tempfile,threading,time,urllib.request
sys.dont_write_bytecode=True
spec=importlib.util.spec_from_file_location('restart_fixture',pathlib.Path(__file__).with_name('dumate-restart-native-probe.py'))
fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)
APP=fixture.APP;KEY=fixture.KEY
class Fixture(fixture.Fixture):
 reject_models=False
 def do_POST(self):
  size=int(self.headers.get('Content-Length','0'));raw=self.rfile.read(size);original=self.rfile;self.payload=json.loads(raw or '{}');self.rfile=io.BytesIO(raw)
  try:
   if self.reject_models and 'chat/completions' in self.path:
    self.calls.append({'path':self.path,'body':self.payload,'auth':self.headers.get('Authorization'),'session_id':self.headers.get('X-Dumate-Session-Id')});self.send_response(400);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(b'{"error":{"message":"isolated upstream failure"}}');return
   super().do_POST()
  finally:self.rfile=original
 def reply(self,body,mime='application/json'):
  payload=getattr(self,'payload',{});messages=payload.get('messages',[]);serialized=json.dumps(messages)
  if 'task_type' in serialized and 'ai_assistant' in serialized and isinstance(body,dict) and body.get('choices'):
   body['choices'][0]['message']['content']=json.dumps({'task_type':'ai_assistant','platform':'cloud','model_type':'text'})
  tools=[t.get('function',{}).get('name') for t in payload.get('tools',[])]
  already=any(any(t.get('function',{}).get('name')=='task' for t in m.get('tool_calls',[])) for m in messages)
  if mime=='text/event-stream' and 'task' in tools and 'TM_SPAWN_CHILD' in serialized and not already:
   delta={'role':'assistant','tool_calls':[{'index':0,'id':'fixture_task_call','type':'function','function':{'name':'task','arguments':json.dumps({'subagent_type':'explore','description':'Isolated child fixture','prompt':'TM_CHILD_REPLY: reply briefly to this isolated test.'})}}]}
   chunk={'id':'fixture-child','object':'chat.completion.chunk','created':1,'model':payload.get('model'),'choices':[{'index':0,'delta':delta,'finish_reason':None}]}
   done={**chunk,'choices':[{'index':0,'delta':{},'finish_reason':'tool_calls'}]}
   body='data: '+json.dumps(chunk)+'\n\ndata: '+json.dumps(done)+'\n\ndata: [DONE]\n\n'
  super().reply(body,mime)

ROOT=pathlib.Path(__file__).resolve().parents[2]
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def request(base,path,body=None,method=None,timeout=30):
 req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json','X-Dumate-Inapp-Key':KEY},method=method)
 try:
  with urllib.request.urlopen(req,timeout=timeout) as r:return json.loads(r.read())
 except urllib.error.HTTPError as error:
  error.add_note(error.read(2048).decode(errors='replace'));raise
def wait(base,path,proc):
 for _ in range(100):
  if proc.poll() is not None:raise AssertionError('Native process exited')
  try:return request(base,path,timeout=.5)
  except (OSError,ValueError):time.sleep(.1)
 raise AssertionError('Native readiness timed out')
def run(root,runtime='0'):
 import http.server
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Fixture);threading.Thread(target=server.serve_forever,daemon=True).start()
 upstream=f'http://127.0.0.1:{server.server_port}';configdir=root/'config';pref=configdir/'desktop-main/dumate-app.json';pref.parent.mkdir(parents=True)
 enginecfg=configdir/'opencode/opencode.json';enginecfg.parent.mkdir(parents=True);plugin=configdir/'opencode/plugins/tokenmarket-dumate-unified.js'
 selected=lambda model:{'baseURL':upstream+'/platform/v1','apiKey':'sk-unified-fixture','model':model,'contextLimit':131072,'outputLimit':4096}
 def save(provider):
  pref.write_text(json.dumps({'customLLMProvider':None}))
  fields=[provider.get(k) for k in ['baseURL','apiKey','model','contextLimit','outputLimit']]
  fingerprint=hashlib.sha256(json.dumps(fields,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
  policy={'unified':{'protocol':3,'owner':'cn.tokenmarket','providerFingerprint':fingerprint,'baseURL':provider['baseURL'],'model':provider['model'],'provider':provider}}
  (configdir/'opencode/tokenmarket-unified.json').write_text(json.dumps(policy))
 pref.write_text(json.dumps({'customLLMProvider':selected('fixture-old')}))
 env={'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','HOME':str(root),'TMPDIR':str(root),'LANG':'en_US.UTF-8','DUMATE_INAPP_KEY':KEY,'DUMATE_LOGIN_MODE':'personal','DUMATE_LOGIN_USER_ID':'fixture-user','DUMATE_LOGIN_BCE_ACCOUNT_ID':'fixture-account','DUMATE_LOGIN_USER_NAME':'fixture','DUMATE_USER_ID':'fixture-user','DUMATE_ACCOUNT_ID':'fixture-account','DUMATE_APP_VER':'1.0.89.447','DUMATE_OTEL_ENABLED':'false','DUMATE_OPENCODE_CP_ENABLE':'false','OPENCODE_DISABLE_MODELS_FETCH':'true','OPENCODE_CONFIG':str(enginecfg),'ROUTING_MULTIMODAL_BASE_URL':upstream+'/official/v1','ROUTING_MULTIMODAL_API_KEY':'baidu-media-fixture','ROUTING_CHAT_API_URL':upstream+'/routing/v1/chat/completions','ROUTING_TIMEOUT_MS':'1000','DUMATE_FEAT_ROUTING_REMOTE':'false','DUMATE_FEAT_AIASSISTANT_SWITCH':'true','OPENCODE_NEW_RUNTIME':runtime}
 for name in ['CONFIG','DATA','CACHE','STATE','GLOBCONFIG','GLOBDATA','GLOBCACHE','GLOBSTATE']:
  p=root/name.lower();p.mkdir(exist_ok=True);env['XDG_'+name+'_HOME']=str(p)
 env['XDG_CONFIG_HOME']=str(configdir)
 sandbox=f'(version 1)(allow default)(deny network-outbound)(allow network-outbound (remote ip "localhost:*"))(deny file-read* (subpath "/Users"))(deny file-write*)(allow file-write* (subpath "{root}") (literal "/dev/null"))'
 backendcfg=root/'backend.yml';backendcfg.write_text('web:\n  host: "127.0.0.1"\n  gin_mode: "release"\nlog:\n  loglevel: 1\n  logdir: "'+str(root/'logs')+'"\n')
 stages=[]
 @contextlib.contextmanager
 def launch(label,enabled,remote=False,scheduler=False):
  bp,ep=port(),port();backend=f'http://127.0.0.1:{bp}';engine=f'http://127.0.0.1:{ep}'
  cfg=json.loads((APP/'config/opencode/opencode.json').read_text().replace('{{desk-main-port}}',str(bp)));cfg['autoupdate']=False;cfg['small_model']=cfg['model'];enginecfg.write_text(json.dumps(cfg))
  runenv=dict(env,DUMATE_ENABLE_FEAT_AIASSISTANT='true' if enabled else 'false',DUMATE_FEAT_ROUTING_REMOTE='true' if remote else 'false')
  if scheduler:runenv['DUMATE_SCHEDULER_URL']=backend
  commands=[[str(APP/'extra-resource/backend/bin/dumate-main-server'),'-c',str(backendcfg),'-port',str(bp),'-hostname','127.0.0.1','-user-id','fixture-user','-opencode-base-url',engine,'-opencode-router-url',upstream,'-opencode-workspace-path',str(root),'-log-dir',str(root/'logs')],[str(APP/'extra-resource/opencode/bin/dumate-opencode'),'serve','--port',str(ep),'--print-logs']]
  processes=[]
  with contextlib.ExitStack() as stack:
   try:
    for i,cmd in enumerate(commands):
     log=stack.enter_context((root/(label+'-'+str(i)+'.log')).open('w'));processes.append(subprocess.Popen(['/usr/bin/sandbox-exec','-p',sandbox,*cmd],env=runenv,cwd=root,stdin=subprocess.PIPE,stdout=log,stderr=subprocess.STDOUT))
    wait(backend,'/global/setting/gateway/custom-llm-provider/get',processes[0]);wait(engine,'/global/health',processes[1]);request(engine,'/global/runtime/ready',{});yield engine,backend
   finally:
    for p in reversed(processes):
     if p.poll() is None:p.terminate()
     try:p.wait(timeout=4)
     except subprocess.TimeoutExpired:p.kill();p.wait()
 def prompt(engine,sid,stage,expected,unified=False,expected_path='/platform/v1/chat/completions',expected_key='sk-unified-fixture',text='Reply to this isolated acceptance fixture.'):
  before=len(Fixture.calls);result=request(engine,'/session/'+sid+'/message',{'parts':[{'type':'text','text':text}]})
  assert not result['info'].get('error'),result['info'].get('error')
  assert result['info']['providerID']=='QianfanPersonalQuota',result['info']['providerID']
  assert any(p.get('text')=='BODY:'+expected for p in result['parts']),[(p.get('type'),p.get('text')) for p in result['parts']]
  calls=[c for c in Fixture.calls[before:] if 'chat/completions' in c['path']]
  assert calls and any(len(c['body'].get('tools',[]))>3 for c in calls),'No actual body request'
  assert all(c['body']['model']==expected for c in calls if unified or c['path']=='/platform/v1/chat/completions'),[(c['path'],c['body'].get('model')) for c in calls]
  if stage=='old_task_after_cold_takeover':
   assert any('BODY:fixture-old' in json.dumps(c['body'].get('messages',[])) for c in calls),'Old completed history was dropped'
  if unified:
   assert all(c['path']==expected_path and c['auth']=='Bearer '+expected_key for c in calls)
   assert all(c['body'].get('max_tokens',4096)<=4096 for c in calls)
   body_calls=[c for c in calls if len(c['body'].get('tools',[]))>3]
   assert body_calls and all(c.get('session_id') for c in body_calls),'Native model request lost its session ID'
   if stage!='actual_subagent_tool_execution':assert all(c['session_id']==sid for c in body_calls),'Main session ID changed'
  stages.append({'stage':stage,'body_model':expected,'actual_model_calls':len(calls),'all_calls_pinned':all(c['body']['model']==expected for c in calls)})
 try:
  with launch('before',False) as (engine,_):
   old=request(engine,'/session',{})['id'];prompt(engine,old,'old_task_before_takeover','fixture-old')
  # Model a completed cloud text reply using the native persisted message schema.
  # All content is generated by this fixture, never copied from real user data.
  with sqlite3.connect(root/'data/opencode/opencode.db') as db:
   for mid,raw in db.execute('select id,data from message where session_id=?',(old,)).fetchall():
    value=json.loads(raw)
    if value.get('role')=='assistant' and value.get('finish')=='stop':
     value.update(providerID='ai_assistant',modelID='model-text',routing={'task_type':'ai_assistant','platform':'cloud','model_type':'text','reason':'llm_decision'})
     db.execute('update message set data=? where id=?',(json.dumps(value),mid))
  plugin.parent.mkdir(parents=True,exist_ok=True);plugin.write_text((ROOT/'rust/tokenmarket-core/src/dumate/unified/plugin.js').read_text());save(selected('fixture-one'))
  with launch('unified',True) as (engine,_):
   prompt(engine,old,'old_task_after_cold_takeover','fixture-one',True)
   new=request(engine,'/session',{})['id'];prompt(engine,new,'new_task_after_cold_takeover','fixture-one',True)
   for mime,filename,data in [
    ('image/png','fixture.png','iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aG7cAAAAASUVORK5CYII='),
    ('video/mp4','fixture.mp4','AAAAHGZ0eXBpc29tAAACAGlzb21pc28yYXZjMQ==')]:
    before=len(Fixture.calls);media_session=request(engine,'/session',{})['id']
    media=request(engine,'/session/'+media_session+'/message',{'parts':[{'type':'text','text':'Describe this isolated media fixture briefly.'},{'type':'file','mime':mime,'filename':filename,'url':'data:'+mime+';base64,'+data}]})
    assert not media['info'].get('error'),media['info'].get('error')
    assert media['info']['providerID']=='qianfan-multimodal',media['info']
    calls=[c for c in Fixture.calls[before:] if 'chat/completions' in c['path']]
    official=[c for c in calls if c['body'].get('model')=='model-mm']
    assert official and all(c['path']=='/official/v1/chat/completions' and c['auth']=='Bearer baidu-media-fixture' for c in official),[(c['path'],c['body'].get('model')) for c in calls]
    assert all(c['body']['model']=='fixture-one' for c in calls if c['path']=='/platform/v1/chat/completions')
    stages.append({'stage':mime.split('/')[0]+'_uses_native_official_route','official_model_calls':len(official),'platform_key_on_media':False})
   prompt(engine,old,'actual_subagent_tool_execution','fixture-one',True,text='TM_SPAWN_CHILD: delegate one small check to an explore subagent, then reply.')
   sessions=request(engine,'/session');children=[row['id'] for row in sessions if row.get('parentID')==old];assert children,'No actual child session was created'
   assert any(c.get('session_id') in children for c in Fixture.calls),'Child session lost its own ID'
   before=len(Fixture.calls)
   request(engine,'/session/'+old+'/summarize',{'providerID':'QianfanPersonalQuota','modelID':'glm-5'})
   summary=[c for c in Fixture.calls[before:] if 'chat/completions' in c['path']]
   assert summary and all(c['body']['model']=='fixture-one' and c['path']=='/platform/v1/chat/completions' for c in summary),'Summary bypassed the selected model'
   assert all(c.get('session_id')==old for c in summary),'Summary lost the native session ID'
   stages.append({'stage':'explicit_summary','actual_model_calls':len(summary),'all_calls_pinned':True})
   save(selected('fixture-two'));prompt(engine,old,'saving_without_restart_keeps_active_snapshot','fixture-one',True)
   before=len(Fixture.calls);Fixture.reject_models=True
   try:
    failed=request(engine,'/session/'+old+'/message',{'parts':[{'type':'text','text':'Failure-protection fixture.'}]})
    assert failed['info'].get('error'),'An upstream failure must not silently succeed on another provider'
   finally:Fixture.reject_models=False
   failure_calls=[c for c in Fixture.calls[before:] if 'chat/completions' in c['path']]
   assert failure_calls and all(c['body']['model']=='fixture-one' and c['path']=='/platform/v1/chat/completions' for c in failure_calls)
   stages.append({'stage':'upstream_failure_no_official_fallback','actual_model_calls':len(failure_calls),'all_calls_pinned':True})
  with launch('switched',True) as (engine,_):
   prompt(engine,old,'old_task_after_model_switch_restart','fixture-two',True)
   prompt(engine,new,'newer_task_after_model_switch_restart','fixture-two',True)
  with launch('remote-preflight',True,True) as (engine,_):
   prompt(engine,old,'remote_classifier_old_task_is_local','fixture-two',True)
   prompt(engine,request(engine,'/session',{})['id'],'remote_classifier_new_task_is_local','fixture-two',True)
  with launch('remote-scheduler',True,True,True) as (engine,_):
   prompt(engine,old,'scheduler_remapped_preflight_is_local','fixture-two',True)
  reconnected=selected('fixture-reconnected');reconnected.update(baseURL=upstream+'/second/v1',apiKey='sk-fixture-second');save(reconnected)
  with launch('reconnected',True) as (engine,_):
   prompt(engine,old,'old_task_after_reconnect_restart','fixture-reconnected',True,'/second/v1/chat/completions','sk-fixture-second')
   prompt(engine,request(engine,'/session',{})['id'],'new_task_after_reconnect_restart','fixture-reconnected',True,'/second/v1/chat/completions','sk-fixture-second')
  plugin.unlink();(configdir/'opencode/tokenmarket-unified.json').unlink();pref.write_text(json.dumps({'customLLMProvider':selected('fixture-old')}))
  with launch('cancelled',False) as (engine,_):prompt(engine,old,'cancelled_after_restart_restores_original','fixture-old')
  plugin.write_text((ROOT/'rust/tokenmarket-core/src/dumate/unified/plugin.js').read_text());save(selected('fixture-reconnected'))
  control=configdir/'opencode/tokenmarket-unified.json';broken=json.loads(control.read_text());broken['unified']['providerFingerprint']='0'*64;control.write_text(json.dumps(broken))
  before=len(Fixture.calls);rejected=False
  with launch('bad-policy',True) as (engine,_):
   try:
    rejected_session=request(engine,'/session',{})['id'];bad=request(engine,'/session/'+rejected_session+'/message',{'parts':[{'type':'text','text':'This request must be rejected.'}]})
    rejected=bool(bad['info'].get('error'))
   except urllib.error.HTTPError as error:rejected=error.code>=400
  assert rejected,'Broken control policy must not run a model'
  assert not any('chat/completions' in c['path'] for c in Fixture.calls[before:]),'Broken policy leaked a model request'
  stages.append({'stage':'invalid_policy_fails_closed','actual_model_calls':0,'all_calls_pinned':True})
  engine_log=(root/'unified-1.log').read_text(errors='replace')
  agents=sorted(set(re.findall(r'service=llm[^\n]*?agent=([^\s]+)',engine_log)))
  assert 'title' in agents and 'explore' in agents,agents
  assert 'ai assistant task complete' not in engine_log.lower()
  # The Go backend must never receive an AI-assistant inference run in managed stages.
  for path in (root/'logs').glob('*Gateway*'):
   assert '/ai_assistant/run' not in path.read_text(errors='replace'),'A cloud inference route escaped the adapter'
  return {'runtime_mode':runtime,'stages':stages,'synthetic_completed_cloud_text_history_preserved':True,'observed_agents':agents,'external_network':'denied','real_user_data':'denied','real_app_restarted':False}
 finally:server.shutdown();server.server_close()
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--runtime',choices=['0','1'],default='0');args=parser.parse_args()
 root=pathlib.Path(tempfile.mkdtemp(prefix='tokenmarket-unified-native-',dir='/private/tmp'))
 try:print(json.dumps(run(root,args.runtime)),flush=True)
 except BaseException:
  print('Isolated diagnostic directory:',root,flush=True);raise
