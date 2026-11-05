const {pathToFileURL} = require('node:url');
const path = require('node:path');
module.exports = async function openFixture(browser, options = {}) {
  const context = await browser.newContext({viewport:{width:900,height:760},colorScheme:options.colorScheme || 'light'});
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(options => {
    if (options.storedTheme !== undefined && localStorage.getItem('tokenmarket.theme') === null) localStorage.setItem('tokenmarket.theme', options.storedTheme);
    const user = {id:'user-fixture',name:'界面测试账号',user_name:'界面测试账号',display_name:'界面测试账号',role:'USER',avatar_url:'avatar_0'};
    const personal = {id:'personal',name:'个人空间',type:'PERSONAL',defaultWorkspace:true};
    const team = {id:'team',name:'团队空间',type:'TEAM',status:'ACTIVE',roleCode:'TEAM_MEMBER'};
    const fixture = window.__fixture = {calls:[],user,personal,team,current:personal,signedIn:options.signedIn !== false,pending:null,testPassed:true,conflicts:options.conflicts || false,inventoryError:options.inventoryError || false,workspaceError:options.workspaceError || false,events:{}};
    const connection = (id, state = 'configured_untested') => ({state,owner:{id:user.id,name:user.name},boundWorkspace:{id:personal.id,name:personal.name},models:[options.model || 'server-model'],keyId:'receipt-'+id,protocol:'OPENAI_CHAT_COMPLETIONS',testResult:null});
    fixture.entries = [['claude-code','Claude Code'],['codex','Codex'],['workbuddy','WorkBuddy'],['deepseek-harness','DeepSeek Harness'],['openclaw','OpenClaw'],['opencode','OpenCode'],['grok-build','Grok Build'],['hermes','Hermes'],['pi','Pi'],['minimax-code','MiniMax Code'],['oh-my-pi','Oh My Pi'],['kimi-code','Kimi Code'],['dumate','百度搭子 DuMate（个人版）'],['dumate-enterprise','百度搭子 DuMate（企业版）']].map(([id,name],i) => ({id,name,installed:i<3,canConnect:i<3,connection:i<2?connection(id,i===0?'test_passed':options.changed?'configuration_changed':'configured_untested'):null}));
    if (options.multi) fixture.entries[2].connection = {...connection('workbuddy'),models:['server-model-a','server-model-b']};
    const installVersions = {'claude-code':'2.1.309',codex:'0.166.0',workbuddy:'5.9.3','deepseek-harness':'0.2.1-rc.5',openclaw:'2026.10.9',opencode:'1.19.5','grok-build':'1.0.70',hermes:'0.22.5',pi:'1.4.0','minimax-code':'0.7.2','oh-my-pi':'18.15.0','kimi-code':'2.4.1',dumate:'1.0.89','dumate-enterprise':'1.0.89'};
    fixture.entries.forEach(entry => {
      if ((options.installedTools || []).includes(entry.id)) {entry.installed=true;entry.canConnect=true;}
      const supported=Boolean(installVersions[entry.id]) && options.unsupportedTool !== entry.id;
      entry.installation = {supported,reason:supported?null:'当前系统暂无兼容安装方案',version:installVersions[entry.id]||null};
      if (options.missingNativeCli && ['claude-code','codex'].includes(entry.id)) { entry.installed=false;entry.canConnect=false;entry.connection=null; }
      if (options.missingGui && ['workbuddy','deepseek-harness'].includes(entry.id)) { entry.installed=false;entry.canConnect=false;entry.connection=null; }
    });
    const plannedMethods = {'claude-code':'通过官方安装器安装',codex:'通过官方安装器安装',workbuddy:'通过官方桌面安装包安装','deepseek-harness':'通过官方桌面安装包安装',openclaw:'通过官方脚本和 npm 安装',opencode:'下载官方独立安装包','grok-build':'通过 npm 安装（含运行环境）',hermes:'通过官方安装脚本安装',pi:'下载官方独立安装包','minimax-code':'通过 npm 安装官方发行包','oh-my-pi':'下载官方独立安装包','kimi-code':'下载官方独立安装包',dumate:'通过官方桌面安装包安装','dumate-enterprise':'通过官方桌面安装包安装'};
    fixture.entries.forEach(entry => {
      const defaults={ 'claude-code':{path:'/Users/fixture/.local/bin/claude',method:'原生程序'},codex:{path:'/Users/fixture/.npm-global/bin/codex',method:'npm'},workbuddy:{path:'/Applications/WorkBuddy.app',method:'桌面应用'}};
      entry.installations=options.installations?.[entry.id] || (entry.installed ? [{...(defaults[entry.id] || {path:`/Users/fixture/.tokenmarket/tools/${entry.id}/bin/tool`,method:'客户端管理'}),selected:true,resolvedPath:null}] : []);
      entry.installationMethod=entry.installations.find(p=>p.selected)?.method || plannedMethods[entry.id];
      entry.configPath=options.configPaths?.[entry.id] || (entry.id==='codex'?'/Users/fixture/.codex/config.toml':`/Users/fixture/.config/${entry.id}/config.json`);
      entry.configDirectory=entry.configPath.slice(0,entry.configPath.lastIndexOf('/'));
      entry.configExists=options.configExists?.[entry.id] ?? false;entry.configDirectoryExists=entry.configExists;
    });
    fixture.installation = null;
    fixture.updateVersions = {...installVersions, ...(options.updateVersions || {})};
    const upgradeEligible = id => options.upgradeTool === id || (options.upgradeTools || []).includes(id);
    const releaseVersions = {...installVersions,...(options.latestVersions || {})};
    fixture.toolUpdates = () => fixture.entries.map(entry => {
      const latest=options.newerRelease===entry.id ? '99.0.0' : releaseVersions[entry.id];
      const canUpgrade=entry.installed && (upgradeEligible(entry.id) || options.newerRelease===entry.id) && fixture.updateVersions[entry.id]!==latest;
      return {tool:entry.id,currentVersion:entry.installed ? fixture.updateVersions[entry.id] : null,targetVersion:options.updateFailure===entry.id ? null : latest,latestVersion:options.updateFailure===entry.id ? null : latest,manualUpgrade:entry.installed && options.manualUpgrade===entry.id,canUpgrade:canUpgrade && options.manualUpgrade!==entry.id && options.updateFailure!==entry.id,state:!entry.installed ? 'not_installed' : options.updateFailure===entry.id ? 'check_failed' : options.manualUpgrade===entry.id ? 'manual_update' : canUpgrade ? 'available' : 'current',note:options.updateFailure===entry.id ? '官方版本检查失败，可稍后重试' : options.manualUpgrade===entry.id ? '请通过原安装渠道更新至官方最新版' : '官方最新版'};
    });
    fixture.windowPreferences = {run_in_background:options.runInBackground !== false,background_available:options.backgroundAvailable !== false,close_notice_pending:Boolean(options.closeNoticePending),error:null};
    if (options.legacy) fixture.entries[3].connection = {state:'legacy',owner:null,boundWorkspace:null,keyId:null,protocol:null,models:['legacy-model']};
    const clone = value => structuredClone(value);
    const loginStatus = () => ({context:fixture.signedIn?{user:{id:user.id,name:user.name},default_workspace:fixture.current}:null,pending:fixture.pending});
    const workspace = () => ({user,current_workspace:fixture.current,switchable_workspaces:[personal,team],balance:options.noBalance?null:{currency:'CNY',amount:'128.60'},balance_error:options.noBalance?'读取余额失败':null,can_recharge:!options.restricted,menus:options.restricted?[]:[{path:'/console/tokens'},{path:'/console/billing'},{path:'/console/spaces'}]});
    fixture.emit = (name,payload) => (fixture.events[name]||[]).forEach(fn=>fn({payload}));
    fixture.loginStatus = loginStatus;
    fixture.clientInstallBusy=Boolean(options.clientInstallBusy);
    fixture.clientUpdateStatus=options.startupClientUpdate || {status:'idle',currentVersion:'1.0.0',availableVersion:null,releaseNotes:null,error:null};
    window.__TAURI__ = {window:{getCurrentWindow:()=>({setTheme:async value=>{fixture.nativeTheme=value;}})},event:{listen:async (name,fn) => { (fixture.events[name] ||= []).push(fn); return () => {}; }},core:{invoke:async (command,args = {}) => {
      fixture.calls.push({command,args:clone(args)});
      const entry = fixture.entries.find(t=>t.id===args.tool);
      if (options.credentialLost && ['connect_tool','reconnect_tool'].includes(command) && !args.recovery) throw {message:'本机保存的连接信息已丢失',recovery:{keyId:'missing-fixture',ownerId:user.id,workspaceId:fixture.current.id,ownerName:user.name,workspaceName:fixture.current.name}};
      if (args.recovery && (args.recovery.ownerId!==user.id || args.recovery.workspaceId!==fixture.current.id)) throw Error('账号或空间已改变');
      switch(command) {
        case 'desktop_client_update_status': {
          const value=clone(fixture.clientUpdateStatus);
          if(options.deferClientUpdateStatus) return new Promise(resolve=>fixture.resolveClientUpdateStatus=()=>resolve(value));
          return value;
        }
        case 'download_client_update': if(options.deferClientDownload)return new Promise(resolve=>fixture.resolveClientDownload=resolve);return {status:'ready',currentVersion:'1.0.0',availableVersion:args.version,releaseNotes:'保留已有连接',error:null};
        case 'acknowledge_client_update_failure': fixture.clientUpdateStatus.showRecoveryNotice=false;return clone(fixture.clientUpdateStatus);
        case 'open_client_update_recovery': return null;
        case 'install_client_update': return fixture.clientInstallBusy ? {status:'ready',currentVersion:'1.0.0',availableVersion:args.version,installAvailable:true,error:'工具连接操作正在进行，请完成后重试安装客户端更新'} : {status:'handoff',currentVersion:'1.0.0',availableVersion:args.version,installAvailable:true,error:null};
        case 'defer_client_update': fixture.clientUpdateStatus.remind=false;return clone(fixture.clientUpdateStatus);
        case 'ignore_client_update_version': fixture.clientUpdateStatus.remind=false;fixture.ignoredClientUpdate=args.version;return clone(fixture.clientUpdateStatus);
        case 'check_client_update': return clone(fixture.clientUpdate || options.clientUpdate || {status:'current',currentVersion:'1.0.0',availableVersion:null,releaseNotes:null,error:null});
        case 'desktop_window_preferences': if(fixture.deferPreferences)return new Promise(resolve=>{fixture.resolvePreferences=()=>resolve(clone(fixture.windowPreferences));});return clone(fixture.windowPreferences);
        case 'save_desktop_window_preferences': if(options.preferenceSaveError)throw Error('无法保存应用设置');fixture.windowPreferences.run_in_background=args.runInBackground;return clone(fixture.windowPreferences);
        case 'acknowledge_desktop_close': if(options.preferenceSaveError)throw Error('无法保存应用设置');fixture.windowPreferences.close_notice_pending=false;fixture.hidden=true;return null;
        case 'cancel_desktop_close': fixture.windowPreferences.close_notice_pending=false;return null;
        case 'quit_desktop_app': fixture.quit=true;return null;
        case 'check_tool_updates': if(fixture.updateError || options.updateError)throw Error('官方版本服务暂不可用');if(fixture.deferUpdates || options.deferUpdates)return new Promise(resolve=>fixture.resolveUpdates=()=>resolve(clone(fixture.toolUpdates())));return clone(fixture.toolUpdates());
        case 'start_tool_upgrade': if(args.targetVersion!==fixture.toolUpdates().find(u=>u.tool===args.tool).targetVersion)throw Error('官方版本信息已改变，请重新检查');fixture.installation={tool:args.tool,stage:'installing',detail:'正在升级至官方最新版',output:['保留原有连接'],installed:true,upgrade:true};return clone(fixture.installation);
        case 'start_tool_installation': fixture.installation={tool:args.tool,stage:options.guiInstall?'waiting_for_user':'dependencies',detail:options.guiInstall?'请完成原生安装器中的步骤；取消请在原生安装器中操作':'正在准备运行依赖',output:['准备工具运行依赖'],installed:false}; return clone(fixture.installation);
        case 'tool_installation_status': if(fixture.installation.stage !== 'cancelled' && !options.installPending && !options.installFail && (!options.guiInstall || fixture.nativeInstalled)){fixture.installation={...fixture.installation,stage:'succeeded',detail:'安装完成，请单独接入',installed:true};entry.installed=true;entry.canConnect=true;if(fixture.installation.upgrade)fixture.updateVersions[args.tool]=releaseVersions[args.tool];} if(options.installFail || options.upgradeFailTool === args.tool)fixture.installation={...fixture.installation,stage:'failed',detail:'依赖下载失败，请重试'}; return clone(fixture.installation);
        case 'cancel_tool_installation': fixture.installation={...fixture.installation,stage:'cancelled',detail:options.guiInstall?'已停止等待；请在原生安装器中取消安装':'安装已取消，已准备的依赖保留'}; return null;
        case 'desktop_login_status': case 'refresh_desktop_session': return clone(loginStatus());
        case 'desktop_workspace_context': if(fixture.workspaceError)throw Error('空间读取失败');return clone(workspace());
        case 'desktop_tool_inventory': if(fixture.inventoryError)throw Error('工具读取失败');if(fixture.deferInventory)return new Promise(resolve=>fixture.resolveInventory=()=>resolve(clone(fixture.entries)));return clone(fixture.entries);
        case 'switch_desktop_workspace': fixture.current = args.workspaceId==='team'?team:personal;return clone(workspace());
        case 'prepare_tool_connect': return null;
        case 'connect_tool': entry.connection=connection(entry.id);if(entry.id==='dumate-enterprise')entry.connection.models=['server-model-a','server-model-b'];entry.connection.boundWorkspace={id:fixture.current.id,name:fixture.current.name};return clone(entry.connection);
        case 'prepare_tool_reconnect': return {oldOwner:clone(entry.connection.owner),oldWorkspace:clone(entry.connection.boundWorkspace),currentOwner:{id:user.id,name:user.name},currentWorkspace:{id:fixture.current.id,name:fixture.current.name}};
        case 'reconnect_tool': entry.connection={...connection(entry.id),boundWorkspace:{id:fixture.current.id,name:fixture.current.name}};return {connection:clone(entry.connection),keyReused:true};
        case 'test_tool_connection': entry.connection.state=fixture.testPassed?'test_passed':'test_failed';entry.connection.testResult={model:args.request.model,detail:'隔离夹具响应',testedAt:'2026-10-29T00:00:00Z'};return {passed:fixture.testPassed,detail:'隔离夹具响应'};
        case 'disconnect_tool': if(fixture.conflicts && !args.request.revision)return {state:'conflicts',revision:'receipt-revision',conflicts:[{file:0,field:1}]};entry.connection=null;return {state:'disconnected',keyRetained:true};
        case 'launch_connected_tool': case 'download_tool': case 'open_desktop_account_page': case 'reopen_desktop_login': return null;
        case 'logout_desktop_session': fixture.signedIn=false;return {};
        case 'begin_desktop_login': fixture.pending={authorize_url:'https://example.invalid/login'};return clone(fixture.pending);
        case 'cancel_desktop_login': fixture.pending=null;return null;
        default: throw Error('Unexpected native command: '+command);
      }
    }}};
  }, options);
  await page.goto(pathToFileURL(path.resolve(__dirname,'../../src/tauri-ui/index.html')).href);
  if (options.signedIn !== false) await page.locator('.tool-row').first().waitFor();
  return {page,errors,close:()=>context.close()};
}
