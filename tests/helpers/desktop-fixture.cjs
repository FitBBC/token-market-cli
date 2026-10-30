const {pathToFileURL} = require('node:url');
const path = require('node:path');
module.exports = async function openFixture(browser, options = {}) {
  const context = await browser.newContext({viewport:{width:900,height:760}});
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(options => {
    const user = {id:'user-fixture',name:'界面测试账号',user_name:'界面测试账号',display_name:'界面测试账号',role:'USER',avatar_url:'avatar_0'};
    const personal = {id:'personal',name:'个人空间',type:'PERSONAL',defaultWorkspace:true};
    const team = {id:'team',name:'团队空间',type:'TEAM',status:'ACTIVE',roleCode:'TEAM_MEMBER'};
    const fixture = window.__fixture = {calls:[],user,personal,team,current:personal,signedIn:options.signedIn !== false,pending:null,testPassed:true,conflicts:options.conflicts || false,inventoryError:options.inventoryError || false,workspaceError:options.workspaceError || false,events:{}};
    const connection = (id, state = 'configured_untested') => ({state,owner:{id:user.id,name:user.name},boundWorkspace:{id:personal.id,name:personal.name},models:[options.model || 'server-model'],keyId:'receipt-'+id,protocol:'OPENAI_CHAT_COMPLETIONS',testResult:null});
    fixture.entries = [['claude-code','Claude Code'],['codex','Codex CLI'],['workbuddy','WorkBuddy'],['deepseek-harness','DeepSeek Harness'],['openclaw','OpenClaw'],['opencode','OpenCode']].map(([id,name],i) => ({id,name,installed:i<3,canConnect:i<3,connection:i<2?connection(id,i===0?'test_passed':options.changed?'configuration_changed':'configured_untested'):null}));
    if (options.multi) fixture.entries[2].connection = {...connection('workbuddy'),models:['server-model-a','server-model-b']};
    if (options.legacy) fixture.entries[3].connection = {state:'legacy',owner:null,boundWorkspace:null,keyId:null,protocol:null,models:['legacy-model']};
    const clone = value => structuredClone(value);
    const loginStatus = () => ({context:fixture.signedIn?{user:{id:user.id,name:user.name},default_workspace:fixture.current}:null,pending:fixture.pending});
    const workspace = () => ({user,current_workspace:fixture.current,switchable_workspaces:[personal,team],balance:options.noBalance?null:{currency:'CNY',amount:'128.60'},balance_error:options.noBalance?'读取余额失败':null,can_recharge:!options.restricted,menus:options.restricted?[]:[{path:'/console/tokens'},{path:'/console/billing'},{path:'/console/spaces'}]});
    fixture.emit = (name,payload) => (fixture.events[name]||[]).forEach(fn=>fn({payload}));
    fixture.loginStatus = loginStatus;
    window.__TAURI__ = {event:{listen:async (name,fn) => { (fixture.events[name] ||= []).push(fn); return () => {}; }},core:{invoke:async (command,args = {}) => {
      fixture.calls.push({command,args:clone(args)});
      const entry = fixture.entries.find(t=>t.id===args.tool);
      if (options.credentialLost && ['connect_tool','reconnect_tool'].includes(command) && !args.recovery) throw {message:'本机保存的连接信息已丢失',recovery:{keyId:'missing-fixture',ownerId:user.id,workspaceId:fixture.current.id,ownerName:user.name,workspaceName:fixture.current.name}};
      if (args.recovery && (args.recovery.ownerId!==user.id || args.recovery.workspaceId!==fixture.current.id)) throw Error('账号或空间已改变');
      switch(command) {
        case 'desktop_login_status': case 'refresh_desktop_session': return clone(loginStatus());
        case 'desktop_workspace_context': if(fixture.workspaceError)throw Error('空间读取失败');return clone(workspace());
        case 'desktop_tool_inventory': if(fixture.inventoryError)throw Error('工具读取失败');if(fixture.deferInventory)return new Promise(resolve=>fixture.resolveInventory=()=>resolve(clone(fixture.entries)));return clone(fixture.entries);
        case 'switch_desktop_workspace': fixture.current = args.workspaceId==='team'?team:personal;return clone(workspace());
        case 'prepare_tool_connect': return null;
        case 'connect_tool': entry.connection=connection(entry.id);entry.connection.boundWorkspace={id:fixture.current.id,name:fixture.current.name};return clone(entry.connection);
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
