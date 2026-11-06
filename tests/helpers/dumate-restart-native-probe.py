#!/usr/bin/env python3
"""Task-state and shutdown checks against the official isolated DuMate engine.

Uses fake model requests, temporary XDG paths, no external network and no real
DuMate user data. --quit-fixture additionally verifies the production AppKit quit
script against our own temporary accessory app, never the user's DuMate app.
"""
import argparse,concurrent.futures,http.server,json,os,pathlib,plistlib,re,signal,socket,subprocess,tempfile,threading,time,urllib.request,urllib.parse
APP=pathlib.Path('/Applications/DuMate.app/Contents/Resources')
KEY='a'*64

class Fixture(http.server.BaseHTTPRequestHandler):
    calls = []
    hold_next_body = False
    body_started = threading.Event()
    release_body = threading.Event()

    def log_message(self, *args):
        pass

    def reply(self, body, mime='application/json'):
        raw = json.dumps(body).encode() if mime == 'application/json' else body.encode()
        self.send_response(200)
        self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(raw)))
        self.send_header('Connection', 'close')
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        self.reply({'success': True, 'data': []})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))) or '{}')
        self.calls.append({'path': self.path, 'body': body, 'auth': self.headers.get('Authorization')})
        if len(body.get('tools', [])) > 3 and Fixture.hold_next_body:
            Fixture.hold_next_body = False
            Fixture.body_started.set()
            assert Fixture.release_body.wait(15), 'Fixture body not released'
        model = body.get('model', '')
        text = 'BODY:' + model
        routing = '/routing/' in self.path
        if routing:
            text = json.dumps({'task_type': 'ai_assistant', 'platform': 'cloud', 'model_type': 'text'})
        if 'chat/completions' not in self.path:
            self.reply({'success': True, 'data': {}})
            return
        if not body.get('stream'):
            self.reply({'id': 'fixture', 'object': 'chat.completion', 'created': 1, 'model': model,
                        'choices': [{'index': 0, 'message': {'role': 'assistant', 'content': text}, 'finish_reason': 'stop'}],
                        'usage': {'prompt_tokens': 1, 'completion_tokens': 1, 'total_tokens': 2}})
            return
        delta = {'role': 'assistant', 'content': text}
        finish = 'stop'
        tools = body.get('tools', [])
        if len(tools) <= 3 and any(t.get('function', {}).get('name') == 'phase-change' for t in tools):
            delta = {'role': 'assistant', 'tool_calls': [{'index': 0, 'id': 'call_' + str(len(self.calls)),
                     'type': 'function', 'function': {'name': 'phase-change',
                     'arguments': json.dumps({'nextPhase': 'direct-execution'})}}]}
            finish = 'tool_calls'
        chunks = [
            {'id': 'fixture', 'object': 'chat.completion.chunk', 'created': 1, 'model': model,
             'choices': [{'index': 0, 'delta': delta, 'finish_reason': None}]},
            {'id': 'fixture', 'object': 'chat.completion.chunk', 'created': 1, 'model': model,
             'choices': [{'index': 0, 'delta': {}, 'finish_reason': finish}],
             'usage': {'prompt_tokens': 1, 'completion_tokens': 1, 'total_tokens': 2}},
        ]
        self.reply(''.join('data: ' + json.dumps(chunk) + '\n\n' for chunk in chunks) + 'data: [DONE]\n\n', 'text/event-stream')

def free_port():
    with socket.socket() as sock:sock.bind(('127.0.0.1',0));return sock.getsockname()[1]
def request(base,path,body=None):
    req=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None,headers={'Content-Type':'application/json','X-Dumate-Inapp-Key':KEY})
    with urllib.request.urlopen(req,timeout=10) as response:return json.loads(response.read())
def verify_activity(root):
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Fixture);threading.Thread(target=server.serve_forever,daemon=True).start()
    port=free_port();base=f'http://127.0.0.1:{port}'
    env={'PATH':'/usr/bin:/bin','HOME':str(root),'TMPDIR':str(root),'LANG':'en_US.UTF-8','DUMATE_INAPP_KEY':KEY,'DUMATE_LOGIN_MODE':'personal','DUMATE_ENABLE_FEAT_AIASSISTANT':'false','OPENCODE_NEW_RUNTIME':'0','DUMATE_APP_VER':'1.0.89.447','DUMATE_OTEL_ENABLED':'false','DUMATE_CLOUD_SYNC':'false','OPENCODE_DISABLE_MODELS_FETCH':'true','ROUTING_CHAT_API_URL':f'http://127.0.0.1:{server.server_port}/routing/v1/chat/completions'}
    for group in ['CONFIG','DATA','CACHE','STATE']:
        directory=root/group.lower();directory.mkdir();env['XDG_'+group+'_HOME']=str(directory)
    config=json.loads((APP/'config/opencode/opencode.json').read_text().replace('{{desk-main-port}}',str(server.server_port)))
    config['provider']['QianfanPersonalQuota']['options']['apiKey']='sk-restart-fixture'
    config['autoupdate']=False;config['small_model']=config['model']
    config_file=root/'config/opencode.json';config_file.write_text(json.dumps(config));env['OPENCODE_CONFIG']=str(config_file)
    policy=f'(version 1)(allow default)(deny network-outbound)(allow network-outbound (remote ip "localhost:*"))(deny file-read* (subpath "/Users"))(deny file-write*)(allow file-write* (subpath "{root}") (literal "/dev/null"))'
    with (root/'engine.log').open('w') as log:
        process=subprocess.Popen(['/usr/bin/sandbox-exec','-p',policy,str(APP/'extra-resource/opencode/bin/dumate-opencode'),'serve','--port',str(port)],cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
        try:
            for _ in range(60):
                try:
                    if request(base,'/global/health').get('healthy'):break
                except OSError:pass
                time.sleep(.1)
            else:raise AssertionError('Engine did not start')
            assert request(base,'/sandbox/has-active-session')['hasActiveSession'] is False
            first=root/'project-a';second=root/'project-b';first.mkdir();second.mkdir()
            query='?directory='+urllib.parse.quote(str(first));other='?directory='+urllib.parse.quote(str(second))
            session=request(base,'/session'+query,{})['id']
            Fixture.body_started.clear();Fixture.release_body.clear();Fixture.hold_next_body=True
            with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
                future=executor.submit(request,base,'/session/'+session+'/message'+query,{'parts':[{'type':'text','text':'Reply to this isolated fixture.'}]})
                assert Fixture.body_started.wait(8),'Body never became busy'
                try:
                    assert request(base,'/sandbox/has-active-session')['hasActiveSession'] is True
                    assert request(base,'/session/status'+other)=={},'Other project should be locally idle'
                    assert request(base,'/sandbox/has-active-session'+other)['hasActiveSession'] is True,'Global check missed another project'
                finally:Fixture.release_body.set()
                result=future.result(timeout=10);assert not result['info'].get('error'),result['info'].get('error')
            for _ in range(50):
                if request(base,'/sandbox/has-active-session')['hasActiveSession'] is False:break
                time.sleep(.05)
            else:raise AssertionError('Engine did not return to idle')
            shutdown=urllib.request.Request(base+'/global/shutdown',data=b'{}',headers={'Content-Type':'application/json','X-Dumate-Inapp-Key':KEY})
            try:
                with urllib.request.urlopen(shutdown,timeout=12) as response:assert response.status==200
            except OSError:
                pass  # A server may exit before the successful shutdown response is flushed.
            assert process.wait(timeout=12)==0,'Native shutdown must exit cleanly'
            return {'official_engine_shutdown_api_exit_verified':True,'official_engine_idle_busy_idle_verified':True,'global_check_includes_other_projects':True,'external_network':'denied','real_user_data':'denied','real_app_restarted':False}
        finally:
            Fixture.release_body.set()
            if process.poll() is None:process.terminate()
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:process.kill();process.wait()
            server.shutdown();server.server_close()
def verify_quit(root):
    app=root/'Restart Fixture.app';contents=app/'Contents';macos=contents/'MacOS';macos.mkdir(parents=True)
    bundle_id='cn.tokenmarket.restart-fixture'
    with (contents/'Info.plist').open('wb') as output:plistlib.dump({'CFBundleIdentifier':bundle_id,'CFBundleExecutable':'fixture','CFBundleName':'Token Market Restart Fixture','CFBundlePackageType':'APPL','CFBundleVersion':'1','LSUIElement':True},output)
    swift=root/'fixture.swift'
    swift.write_text("""import AppKit
final class Delegate:NSObject, NSApplicationDelegate {
 func applicationShouldTerminate(_ sender:NSApplication)->NSApplication.TerminateReply {
  let marker=Bundle.main.bundleURL.deletingLastPathComponent().appendingPathComponent("graceful.txt")
  try? "graceful".write(to:marker,atomically:true,encoding:.utf8)
  return .terminateNow
 }
}
let application=NSApplication.shared
let delegate=Delegate()
application.delegate=delegate
application.setActivationPolicy(.accessory)
application.run()
""")
    subprocess.run(['/usr/bin/swiftc',str(swift),'-o',str(macos/'fixture')],check=True,capture_output=True)
    source=(pathlib.Path(__file__).resolve().parents[2]/'rust/tokenmarket-core/src/dumate/restart.rs').read_text()
    quit_script=re.search(r'let script\s*=\s*r#"(.*?)"#;',source,re.S).group(1)
    read_script="ObjC.import('AppKit');function run(argv){var apps=$.NSRunningApplication.runningApplicationsWithBundleIdentifier(argv[0]);for(var i=0;i<Number(apps.count);i++){var app=apps.objectAtIndex(i);if(ObjC.unwrap(app.bundleURL.path)===argv[1])return String(Number(app.processIdentifier));}return '0';}"
    pid=0
    try:
        subprocess.run(['/usr/bin/open','-g','-n','-a',str(app)],check=True,capture_output=True)
        for _ in range(30):
            pid=int(subprocess.check_output(['/usr/bin/osascript','-l','JavaScript','-e',read_script,'--',bundle_id,str(app)],text=True).strip())
            if pid:break
            time.sleep(.1)
        assert pid,'Fixture application not found'
        output=subprocess.check_output(['/usr/bin/osascript','-l','JavaScript','-e',quit_script,'--',str(app),str(pid),bundle_id],text=True).strip()
        assert output=='requested',output
        for _ in range(50):
            if (root/'graceful.txt').exists():break
            time.sleep(.1)
        assert (root/'graceful.txt').read_text()=='graceful','Normal application termination delegate was not called'
        return {'production_quit_script_verified_on_own_fixture':True,'normal_termination_delegate_called':True,'user_dumate_process_untouched':True}
    finally:
        if pid:
            try:
                executable=subprocess.check_output(['/bin/ps','-p',str(pid),'-o','comm='],text=True).strip()
                if executable==str(macos/'fixture'):os.kill(pid,signal.SIGTERM)
            except (OSError,subprocess.CalledProcessError):pass
        register=pathlib.Path('/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister')
        if register.exists():subprocess.run([str(register),'-u',str(app)],capture_output=True)
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--quit-fixture',action='store_true');args=parser.parse_args()
    with (APP.parent/'Info.plist').open('rb') as source:assert plistlib.load(source)['CFBundleShortVersionString']=='1.0.89'
    with tempfile.TemporaryDirectory(prefix='tokenmarket-restart-probe-',dir='/private/tmp') as temporary:
        root=pathlib.Path(temporary);report=verify_activity(root)
        if args.quit_fixture:report.update(verify_quit(root))
        print(json.dumps(report))
