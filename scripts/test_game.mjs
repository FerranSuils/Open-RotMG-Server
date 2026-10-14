import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, mkdir, writeFile, readFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createServer} from 'node:http';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {fileURLToPath} from 'node:url';

const exec=promisify(execFile), script=fileURLToPath(new URL('./game.mjs',import.meta.url));
async function fixture(t) {
  const dir=await mkdtemp(join(tmpdir(),'openrotmg-game-test-')), calls=[];
  await mkdir(join(dir,'testing'));
  await writeFile(join(dir,'control.token'),'fixture-control');
  await writeFile(join(dir,'testing.json'),JSON.stringify({schema:1,provenance:'prepared'}));
  let paused=true;
  const view=()=>({actor:1,controller:'native',paused,position:{x:1,y:1},session:{world:-1},lifecycle:{alive:true},character:{hp:100,mp:50,items:[]},verification:{shots:0}});
  let origin;
  const server=createServer(async(req,res)=>{
    res.setHeader('Content-Type','application/json');
    if(req.url==='/json/list')return res.end(JSON.stringify([{id:'owned-fixture',url:origin+'/',webSocketDebuggerUrl:'ws://127.0.0.1/fixture'}]));
    assert.equal(req.headers.authorization,'Bearer fixture-control');
    const chunks=[];for await(const chunk of req)chunks.push(chunk);
    const request=JSON.parse(Buffer.concat(chunks));calls.push(request);
    const c=request.command;
    if(c.op==='pause')paused=true;if(c.op==='resume')paused=false;
    res.end(JSON.stringify(c.op==='observe'?view():c.op==='run'?{status:'succeeded',result:{goal_reached:true,state:view()}}:{status:'succeeded',paused}));
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const port=server.address().port;origin=`http://127.0.0.1:${port}`;
  const helper=join(dir,'managed-fixture.mjs'), bootstrap=join(dir,'cdp-fixture.mjs'), log=join(dir,'cdp.jsonl');
  await writeFile(helper,"export const scriptOwner=()=> 'synthetic-test-owner'; export const managedCDP=async(port,target,owner,send)=>send;");
  await writeFile(bootstrap,`
    import {appendFileSync} from 'node:fs';
    globalThis.WebSocket=class {
      constructor(){queueMicrotask(()=>this.onopen());}
      send(raw){const row=JSON.parse(raw);appendFileSync(process.env.FIXTURE_LOG,JSON.stringify(row)+'\\n');
        let value=true,error;
        if(row.method==='Runtime.evaluate'&&row.params.expression.includes('getBoundingClientRect'))value={x:0,y:0,width:800,height:600,viewportWidth:800,viewportHeight:600};
        if(process.env.FIXTURE_FAULT==='input'&&row.method==='Input.dispatchKeyEvent'&&row.params.type==='keyDown')error={message:'fixture keydown failure'};
        if(process.env.FIXTURE_FAULT==='pause'&&row.method==='Runtime.evaluate'&&row.params.expression.includes('.suspend()'))error={message:'fixture native pause failure'};
        setTimeout(()=>this.onmessage({data:JSON.stringify({id:row.id,...(error?{error}:{result:row.method==='Runtime.evaluate'?{result:{value}}:{}})})}),0);
      }
      close(){this.onclose?.();}
    };
  `);
  const config=join(dir,'config.json');
  await writeFile(config,JSON.stringify({origin,state:dir,browserTools:helper,native:[{port,target:'owned-fixture',actor:1}]}));
  t.after(async()=>{await new Promise(resolve=>server.close(resolve));await rm(dir,{recursive:true});});
  async function run(id,command,fault='') {
    const path=join(dir,'command.json');await writeFile(path,JSON.stringify({id,command}));
    try {return {...await exec(process.execPath,['--import',bootstrap,script,config,path],{env:{...process.env,FIXTURE_LOG:log,FIXTURE_FAULT:fault},timeout:20000}),code:0};}
    catch(e){return {stdout:e.stdout,stderr:e.stderr,code:e.code};}
  }
  const events=async()=>{try{return(await readFile(log,'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse);}catch(e){if(e.code==='ENOENT')return[];throw e;}};
  return {dir,calls,run,events,paused:()=>paused};
}
const move={op:'act',actor:1,input:{keys:['d'],duration_ms:1},pauseAfter:true};
test('native request releases keys, coordinates pause, and replays without repeated effects',async t=>{
  const f=await fixture(t), first=await f.run('move-once',move);
  assert.equal(first.code,0,first.stderr);
  assert.equal(JSON.parse(first.stdout).pause_scope,'server_and_native');
  const events=await f.events();
  assert.deepEqual(events.filter(e=>e.method==='Input.dispatchKeyEvent').map(e=>e.params.type),['keyDown','keyUp']);
  assert.ok(!events.some(e=>/bringToFront|activateTarget|setWindowBounds/.test(e.method)));
  const before=f.calls.length, second=await f.run('move-once',move);
  assert.equal(second.code,0,second.stderr);
  assert.equal(f.calls.length,before);
  assert.equal((await f.events()).length,events.length);
  assert.equal(f.paused(),true);
});
test('primary input failure retains release and pause cleanup evidence',async t=>{
  const f=await fixture(t), result=await f.run('bad-input',move,'input');
  assert.equal(result.code,1);
  assert.match(result.stderr,/fixture keydown failure/);
  assert.ok((await f.events()).some(e=>e.method==='Input.dispatchKeyEvent'&&e.params.type==='keyUp'));
  assert.ok(f.calls.some(e=>e.command.op==='pause'));
  assert.equal(f.paused(),true);
});
test('unconfirmed native pause still requests server stop and never reports success',async t=>{
  const f=await fixture(t), result=await f.run('bad-pause',move,'pause');
  assert.equal(result.code,1);
  assert.match(result.stderr,/fixture native pause failure/);
  assert.ok(f.calls.some(e=>e.command.op==='pause'&&e.command.native_paused.length===0));
  assert.equal(f.paused(),true);
});
test('unknown input and changed id reuse fail without resuming',async t=>{
  const f=await fixture(t), result=await f.run('bad-fields',{...move,input:{...move.input,immunity:true}});
  assert.equal(result.code,1);
  assert.ok(!f.calls.some(e=>e.command.op==='resume'));
  const first=await f.run('valid',move);assert.equal(first.code,0,first.stderr);
  const before=f.calls.length, conflict=await f.run('valid',{...move,pauseAfter:false});
  assert.equal(conflict.code,1);assert.match(conflict.stderr,/different command/);assert.equal(f.calls.length,before);
});
test('real-controller wrapper retains stable run id and refreshes post-pause state',async t=>{
  const f=await fixture(t), result=await f.run('bounded-goal',{op:'run',actor:1,goal:{kind:'time',ms:100},max_game_ms:1000});
  assert.equal(result.code,0,result.stderr);
  assert.equal(f.calls.find(e=>e.command.op==='run').id,'bounded-goal');
  assert.equal(JSON.parse(result.stdout).result.state.paused,true);
});
