// Run with Node + Playwright installed. All native IPC is isolated in the page;
// these tests never load credentials, change tool files, launch tools or bill a model.
const {test, before, after} = require('node:test');
const assert = require('node:assert/strict');
const {chromium} = require('playwright');
let browser;
before(async () => { browser = await chromium.launch({headless:true, ...(process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL ? {channel:process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL} : {})}); });
after(async () => { await browser?.close(); });
const createFixture = require('./helpers/desktop-fixture.cjs');
const openFixture = options => createFixture(browser, options);
const calls = (page,command) => page.evaluate(command => window.__fixture.calls.filter(call=>call.command===command),command);
async function detail(page,id) { await page.locator(`[data-tool="${id}"] .tool-detail`).click(); }
async function switchTeam(page) { await page.locator('.workspace-switch').click();await page.locator('dialog[open] [data-workspace-id="team"]').click();await page.locator('.workspace-switch').filter({hasText:'团队空间'}).waitFor(); }
async function manage(page) { await page.locator('.tool-menu summary').click(); }

test('real UI renders safe native summaries, searches, navigates and fits desktop/mobile', async () => {
  const f = await openFixture();try {
    const p=f.page;assert.equal(await p.locator('.tool-row').count(),6);
    assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.equal((await calls(p,'connect_tool')).length,0);
    await p.locator('.tools-search').fill('Codex');assert.equal(await p.locator('.tool-row').count(),1);
    await p.locator('.tools-search').fill('no-match');await p.getByRole('heading',{name:'没有匹配的工具'}).waitFor();
    await p.getByRole('button',{name:'查看全部工具'}).click();assert.equal(await p.locator('.tool-row').count(),6);
    await p.locator('[data-tool-filter="available"]').click();assert.equal(await p.locator('.tool-row').count(),1);
    await p.locator('[data-tool-filter="all"]').click();
    for(const width of [1440,900,375]) { await p.setViewportSize({width,height:width===1440?1000:760});assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);assert.equal(await p.locator('#signed-in').isVisible(),true); }
    await p.locator('.primary-nav [data-page="account"]').click();await p.getByRole('heading',{name:'可切换空间'}).waitFor();
    await p.locator('.primary-nav [data-page="help"]').click();await p.getByRole('heading',{name:'常见问题'}).waitFor();assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});
test('new connect requires explicit confirmation, uses selected workspace and server model', async () => {
  const f=await openFixture({model:'model-from-catalogue'});try {
    const p=f.page;await switchTeam(p);assert.match(await p.locator('[data-tool="claude-code"]').innerText(),/个人空间/);
    await p.locator('[data-tool="workbuddy"] .tool-connect').click();await p.locator('.connect-confirm-dialog[open]').waitFor();
    assert.match(await p.locator('.connect-target').innerText(),/团队空间/);assert.equal((await calls(p,'connect_tool')).length,0);
    await p.locator('.connect-cancel').click();assert.equal((await calls(p,'connect_tool')).length,0);
    await p.locator('[data-tool="workbuddy"] .tool-connect').click();await p.locator('.connect-confirm').click();
    await p.getByRole('heading',{name:'WorkBuddy 已接入'}).waitFor();await p.locator('dialog[open] .close').click();
    assert.deepEqual((await calls(p,'connect_tool'))[0].args,{tool:'workbuddy'});
    await detail(p,'workbuddy');assert.match(await p.locator('#tool-detail-region').innerText(),/团队空间/);assert.match(await p.locator('#tool-detail-region').innerText(),/model-from-catalogue/);
    assert.equal((await calls(p,'test_tool_connection')).length,0);await p.locator('#tool-detail-region .tool-launch').click();assert.equal((await calls(p,'launch_connected_tool'))[0].args.tool,'workbuddy');assert.deepEqual(f.errors,[]);
  } finally {await f.close();}
});
test('paid test requires model and confirmation and keeps saved key/workspace after switch', async () => {
  const f=await openFixture({multi:true});try {
    const p=f.page;await switchTeam(p);await detail(p,'workbuddy');await p.locator('.tool-test').click();
    assert.match(await p.locator('.test-context').innerText(),/计费空间：个人空间/);
    assert.equal(await p.locator('.test-confirm').isDisabled(),true);assert.equal((await calls(p,'test_tool_connection')).length,0);
    await p.locator('.test-model').selectOption('server-model-b');await p.locator('.test-confirm').click();await p.locator('.test-feedback.success').waitFor();
    assert.deepEqual((await calls(p,'test_tool_connection'))[0].args,{tool:'workbuddy',expectedKeyId:'receipt-workbuddy',request:{confirmed:true,model:'server-model-b'}});
    await p.locator('.test-cancel').click();await p.evaluate(()=>window.__fixture.testPassed=false);await p.locator('.tool-test').click();await p.locator('.test-model').selectOption('server-model-a');await p.locator('.test-confirm').click();await p.locator('.test-feedback.error').waitFor();assert.match(await p.locator('.test-feedback').innerText(),/失败/);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('configuration drift disables testing and legacy connections retain unknown ownership', async () => {
  const f=await openFixture({changed:true,legacy:true});try {
    const p=f.page;await detail(p,'codex');assert.equal(await p.locator('.tool-test').isDisabled(),true);
    await p.locator('.primary-nav [data-page="tools"]').click();await detail(p,'deepseek-harness');assert.match(await p.locator('#tool-detail-region').innerText(),/未知（旧版记录）/);
    assert.equal(await p.locator('#tool-detail-region .tool-test').count(),0);assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('reconnect preserves native receipt and expected current ownership in command arguments', async () => {
  const f=await openFixture();try {
    const p=f.page;await switchTeam(p);await detail(p,'claude-code');await manage(p);await p.locator('#tool-detail-region .tool-connect').click();
    assert.equal((await calls(p,'reconnect_tool')).length,0);assert.match(await p.locator('.reconnect-from').innerText(),/个人空间/);assert.match(await p.locator('.reconnect-to').innerText(),/团队空间/);
    await p.locator('.reconnect-confirm').click();await p.getByRole('heading',{name:'Claude Code 已接入'}).waitFor();
    assert.deepEqual((await calls(p,'reconnect_tool'))[0].args,{tool:'claude-code',expectedKeyId:'receipt-claude-code',expectedOwnerId:'user-fixture',expectedWorkspaceId:'team'});assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('disconnect confirmation and conflict resolution forward only explicit receipt choices', async () => {
  const f=await openFixture({conflicts:true});try {
    const p=f.page;await detail(p,'claude-code');await manage(p);await p.locator('.tool-disconnect').click();assert.equal((await calls(p,'disconnect_tool')).length,0);
    await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入'}).click();await p.locator('.disconnect-dialog[open]').waitFor();assert.equal(await p.locator('.disconnect-confirm').isDisabled(),true);
    await p.locator('.disconnect-cancel').click();assert.equal((await calls(p,'disconnect_tool')).length,1);
    await manage(p);await p.locator('.tool-disconnect').click();await p.locator('.disconnect-confirm-dialog').getByRole('button',{name:'确认取消接入'}).click();await p.locator('.disconnect-conflicts select').selectOption('keep_current');await p.locator('.disconnect-confirm').click();await p.getByRole('heading',{name:'取消接入完成'}).waitFor();
    assert.deepEqual((await calls(p,'disconnect_tool')).at(-1).args,{tool:'claude-code',expectedKeyId:'receipt-claude-code',request:{revision:'receipt-revision',resolutions:[{file:0,field:1,choice:'keep_current'}]}});assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('native login waiting/cancel and logout preserve tool configuration, late inventory stays cleared', async () => {
  const f=await openFixture({signedIn:false});try {
    const p=f.page;await p.locator('#begin').click();await p.locator('#waiting').waitFor();assert.equal((await calls(p,'begin_desktop_login')).length,1);
    await p.locator('#cancel').click();await p.locator('#begin').waitFor();
    await p.evaluate(()=>{window.__fixture.signedIn=true;window.__fixture.emit('desktop-login-status',window.__fixture.loginStatus());});await p.locator('.tool-row').first().waitFor();
    await p.evaluate(()=>{window.__fixture.deferInventory=true;void window.TokenMarketTools.refresh();});
    await p.locator('.workspace-profile-button').click();await p.locator('.workspace-dialog[open]').getByRole('button',{name:'退出登录',exact:true}).click();await p.locator('#login').waitFor();await p.evaluate(()=>window.__fixture.resolveInventory());await p.waitForTimeout(50);
    assert.equal(await p.locator('.tool-row').count(),0);assert.equal((await calls(p,'disconnect_tool')).length,0);assert.equal((await calls(p,'test_tool_connection')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});
test('permissions and unavailable balance are respected; workspace failure keeps logout accessible', async () => {
  for(const options of [{restricted:true,noBalance:true},{workspaceError:true}]){
    const f=await openFixture(options);try {const p=f.page;
      if(options.restricted){await p.locator('.primary-nav [data-page="account"]').click();assert.equal(await p.getByRole('button',{name:'Token 管理',exact:true}).count(),0);assert.equal(await p.getByRole('button',{name:'充值',exact:true}).count(),0);assert.match(await p.locator('.account-balance').innerText(),/余额暂不可用/);}
      else{await p.locator('.workspace-profile-button').click();await p.locator('.workspace-dialog[open]').getByRole('button',{name:'退出登录',exact:true}).click();await p.locator('#login').waitFor();}assert.deepEqual(f.errors,[]);
    }finally{await f.close();}
  }
});
test('a refreshed workspace invalidates a pending connect before any configuration write', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.locator('[data-tool="workbuddy"] .tool-connect').click();await p.locator('.connect-confirm-dialog[open]').waitFor();
    await p.evaluate(async()=>{window.__fixture.current=window.__fixture.team;await window.TokenMarketWorkspace.refresh();});
    assert.equal(await p.locator('.connect-confirm-dialog').isVisible(),false);assert.equal((await calls(p,'connect_tool')).length,0);assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
});

test('Agent details show recorded configuration files verbatim, refresh disk content and fit narrow views', async () => {
  const raw = '{"apiKey":"sk-unmasked-fixture","note":"<img src=x onerror=alert(1)>"}\n' + 'long-value='.repeat(80);
  const filePath = '/custom/profile/' + 'long-directory/'.repeat(8) + 'settings.json';
  const f = await openFixture({configurationFiles:[{path:filePath,content:raw,error:null},{path:'/custom/credentials.yaml',content:null,error:'文件不存在，可能已被移动或删除'}]});
  try {
    const p=f.page;
    assert.equal((await calls(p,'tool_configuration_files')).length,0);
    await detail(p,'claude-code');await p.locator('.tool-configuration-file').first().waitFor();
    assert.equal(await p.getByText('查看接入信息',{exact:true}).count(),0);
    assert.equal(await p.getByText('协议',{exact:true}).count(),0);
    assert.deepEqual((await calls(p,'tool_configuration_files'))[0].args,{tool:'claude-code',expectedKeyId:'receipt-claude-code'});
    assert.equal(await p.locator('.tool-configuration-file').count(),2);
    const file=p.locator('.tool-configuration-file').first();
    assert.notEqual(await file.getAttribute('open'),null);
    assert.equal(await p.locator('.tool-configuration-content').textContent(),raw);
    assert.equal(await p.locator('.tool-configuration-content img').count(),0);
    assert.match(await p.locator('.tool-configuration-error').innerText(),/文件不存在/);
    for (const width of [1440,900,375]) {
      await p.setViewportSize({width,height:760});
      assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    }
    // Inventory refresh must not discard the user's fold state or reading position.
    await file.locator('summary').click();
    await p.evaluate(()=>window.TokenMarketTools.refresh(true));
    assert.equal(await file.getAttribute('open'),null);
    await p.evaluate(()=>window.__fixture.configurationFiles=[{path:'/custom/profile/settings.json',content:'updated-key=sk-latest',error:null}]);
    await p.getByRole('button',{name:'刷新内容',exact:true}).click();
    await p.waitForFunction(()=>document.querySelector('.tool-configuration-content')?.textContent==='updated-key=sk-latest');
    assert.equal(await p.locator('.tool-configuration-file').count(),1);
    assert.equal((await calls(p,'connect_tool')).length,0);assert.equal((await calls(p,'test_tool_connection')).length,0);
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('configuration read errors can retry and late contents cannot return after logout', async () => {
  const f=await openFixture();try {
    const p=f.page;await p.evaluate(()=>window.__fixture.configurationError='连接已改变，请刷新后重试');
    await detail(p,'codex');await p.getByText('读取失败：Error: 连接已改变，请刷新后重试',{exact:true}).waitFor();
    assert.equal(await p.getByRole('button',{name:'刷新内容'}).isEnabled(),true);
    await p.evaluate(()=>{window.__fixture.configurationError=null;window.__fixture.deferConfiguration=true;window.__fixture.configurationFiles=[{path:'/custom/config.toml',content:'late-private-key',error:null}];});
    await p.getByRole('button',{name:'刷新内容'}).click();
    await p.waitForFunction(()=>Boolean(window.__fixture.resolveConfiguration));
    await p.locator('.workspace-profile-button').click();await p.locator('.workspace-dialog[open]').getByRole('button',{name:'退出登录',exact:true}).click();await p.locator('#login').waitFor();
    await p.evaluate(()=>window.__fixture.resolveConfiguration());await p.waitForTimeout(50);
    assert.equal(await p.locator('#tool-detail-region').textContent(),'');
    assert.equal(await p.getByText('late-private-key',{exact:true}).count(),0);assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('code viewer formats JSON without changing keys or numeric precision and restores the complete original', async () => {
  const raw = '{"apiKey":"sk-plain","large":900719925474099312345,"duplicate":1,"duplicate":2,"items":[true,null,{},[]],"escaped":"a\\n\\\"b"}\r\n';
  const f=await openFixture({configurationFiles:[{path:'/custom/settings.json',content:raw,error:null}]});
  try {
    const p=f.page;await detail(p,'claude-code');
    const viewer=p.locator('.configuration-editor');await viewer.waitFor();
    const content=viewer.locator('.tool-configuration-content');
    assert.equal(await content.textContent(),raw);
    assert.equal(await viewer.locator('.configuration-editor-gutter').textContent(),'1\n2');
    assert.equal(await viewer.locator('.config-token-key').first().textContent(),'"apiKey"');
    assert.equal(await viewer.locator('.config-token-string').first().textContent(),'"sk-plain"');
    assert.equal(await content.evaluate(node=>node.isContentEditable),false);
    const before=await calls(p,'tool_configuration_files');
    await viewer.getByRole('button',{name:'格式化',exact:true}).click();
    const formatted=await content.textContent();
    assert.match(formatted,/\n  "large": 900719925474099312345,/);
    assert.match(formatted,/"duplicate": 1,\n  "duplicate": 2/);
    assert.match(formatted,/"items": \[\n    true,\n    null,\n    \{\},\n    \[\]\n  \]/);
    assert.match(formatted,/"escaped": "a\\n\\\"b"/);
    const lineCount=formatted.split('\n').length;
    assert.equal(await viewer.locator('.configuration-editor-gutter').textContent(),Array.from({length:lineCount},(_,i)=>i+1).join('\n'));
    assert.match(await viewer.locator('.configuration-editor-info').textContent(),new RegExp(`JSON · ${lineCount} 行 · 只读`));
    await p.evaluate(()=>window.TokenMarketTools.refresh(true));
    assert.equal(await content.textContent(),formatted);
    await viewer.getByRole('button',{name:'原文',exact:true}).click();assert.equal(await content.textContent(),raw);
    assert.deepEqual(await calls(p,'tool_configuration_files'),before);
    assert.equal((await calls(p,'connect_tool')).length,0);assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});

test('non-JSON, invalid and empty files retain their full text and have accurate line numbers', async () => {
  const files=[
    {path:'/custom/config.toml',content:'[model_providers.tokensmarket]\nexperimental_bearer_token = "sk-plain"\nmodel = "glm-5.2"\n'},
    {path:'/custom/profile.yml',content:'models:\n  - name: alpha\n# preserve this comment\napiKey: sk-plain'},
    {path:'/custom/opencode.jsonc',content:'{\n  // keep comments\n  "provider": {},\n}\n'},
    {path:'/custom/invalid.json',content:'{"incomplete": [\n'},
    {path:'/custom/empty.json',content:''}
  ].map(file=>({...file,error:null}));
  const f=await openFixture({configurationFiles:files});
  try {
    const p=f.page;await detail(p,'codex');await p.locator('.configuration-editor').last().waitFor();
    for (let i=0;i<files.length;i++) {
      const editor=p.locator('.configuration-editor').nth(i);
      assert.equal(await editor.locator('.tool-configuration-content').textContent(),files[i].content);
      assert.equal(await editor.getByRole('button',{name:'格式化',exact:true}).isDisabled(),true);
      assert.equal(await editor.locator('.configuration-editor-gutter').textContent(),Array.from({length:files[i].content.split('\n').length},(_,i)=>i+1).join('\n'));
    }
    assert.deepEqual(f.errors,[]);
  } finally { await f.close(); }
});
