import {readFile, writeFile, readdir, rename, open} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {setTimeout as wait} from 'node:timers/promises';
import {nativeClock} from './native-clock.mjs';

const help=`Usage: node scripts/game.mjs CONFIG.json REQUEST.json
Start a new isolated prepared server with ./run.sh PORT --test-state NEW_DIRECTORY.
CONFIG: {"origin":"http://127.0.0.1:PORT","state":"/absolute/NEW_DIRECTORY"}
REQUEST: {"id":"unique-stable-id","command":{"op":"capabilities"}}
Reuse the same id only to retrieve the same command's result, never to retry uncertain effects.
Read operations: capabilities; catalog {search,limit}; world_info {world}; observe {actor}; inspect {actor}; result {request}; trace {request,offset,limit}.
Spawn: {op:"spawn",spec:{guid,class,level,seed,equipment:[weapon,ability,armor,ring],backpack:[eight explicit slots],world,position:{x,y},controller:"direct"|"native"|"game_bot",stats:null|[eight explicit stats]}}. Empty slot is -1. IDs and names come from catalog. Spawn never chooses class, level, gear or a world for you.
Direct act: {op:"act",actor,action:{command:...},duration_ms,pauseAfter:true} uses ordinary server actions. run {actor,goal,max_game_ms,pauseAfter:true} delegates only advertised goals to the actual gameplay bot; unsupported classes/goals fail.
Native CONFIG also requires browserTools:"/absolute/managed/direct-cdp.mjs" and native:[{port,target,actor}] for EVERY connected native actor. Native act uses input:{keys:["d"],duration_ms:600,mouse:{x:450,y:300}}; mouse is optional left firing in the unclipped 800x600 client. Keys and durations are bounded. No desktop activation.
pauseAfter defaults to true: the whole isolated session does not progress while you inspect and think. Native pause requires Ruffle suspension, pause-adjusted page clocks and server pause; simulation-only pause is not native acceptance. Initialize while Ruffle is playing; an earlier untracked pause requires normal reconnection of the same living character. Set false explicitly for continuous real-time checks.
Explicit paused mutations: world {template}; cheat {actor,change:{kind:"stats",values}|{kind:"item",slot,item}|{kind:"teleport",position}}. These are recorded cheats, never earned gameplay.
A reached time window is not a dungeon clear. Inspect actual effects and goal_reached. cancel {request} addresses a running server request by stable id. Compact receipts point to bounded evidence in STATE/testing; preserve failed receipts and dead characters.
`;
const [configPath, commandPath] = process.argv.slice(2);
if(configPath==='--help'&&process.argv.length===3){console.log(help);process.exit(0);}
if (!configPath || !commandPath || process.argv.length !== 4) throw new Error(help);
const config = JSON.parse(await readFile(configPath, 'utf8'));
const {id, command} = JSON.parse(await readFile(commandPath, 'utf8'));
if(typeof id!=='string'||!id.length||id.length>64||!/^[A-Za-z0-9_-]+$/.test(id)||!command||typeof command!=='object') throw new Error('Command file requires {id,command}; id must be 1–64 ASCII letters, digits, hyphens or underscores and remain stable on retry');
const origin = new URL(config.origin);
if (origin.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(origin.hostname) || origin.pathname !== '/' || origin.search || origin.hash) throw new Error('Testing requires an explicit loopback HTTP origin');
const token = (await readFile(`${config.state}/control.token`, 'utf8')).trim();
const marker=JSON.parse(await readFile(`${config.state}/testing.json`,'utf8'));
if(marker.schema!==1||marker.provenance!=='prepared') throw new Error('Native bridge requires an explicitly prepared test state');
const path=`${config.state}/testing/${id}.client.json`;
const mutating=!['capabilities','catalog','world_info','observe','inspect','result','trace'].includes(command.op);
const syncDirectory=async()=>{const dir=await open(`${config.state}/testing`,'r');try{await dir.sync();}finally{await dir.close();}};
let ownsIntent=false, mayProgress=false, sequence=0;
if(mutating) {
  let saved;
  try {saved=JSON.parse(await readFile(path,'utf8'));} catch(e){if(e.code!=='ENOENT')throw e;}
  if(saved) {
    if(JSON.stringify(saved.command)!==JSON.stringify(command))throw new Error('Request id was already used for a different command');
    if(saved.status==='running')throw new Error('Request has no confirmed result; inspect server trace and actor state instead of replaying effects');
    console.log(JSON.stringify({...saved.result,evidence:path}));process.exit(saved.result.status==='failed'?1:0);
  }
  if((await readdir(`${config.state}/testing`)).filter(n=>n.endsWith('.client.json')).length>=256)throw new Error('Native receipt budget reached; preserve this evidence and create a new owned test state');
  await writeFile(path,JSON.stringify({command,status:'running'}),{flag:'wx',mode:0o600,flush:true});ownsIntent=true;await syncDirectory();
}
const save=async result=>{if(ownsIntent){await writeFile(`${path}.tmp`,JSON.stringify({command,status:'complete',result}),{flag:'wx',mode:0o600,flush:true});await rename(`${path}.tmp`,path);await syncDirectory();}};
const api = async (value, requestId=`${id}-ctl${++sequence}`) => {
  const response = await fetch(new URL('/dev/testing', origin), {
    method: 'POST', headers: {Authorization:`Bearer ${token}`, 'Content-Type':'application/json'},
    body: JSON.stringify({id:requestId, command:value}),
    signal: AbortSignal.timeout(660000),
  });
  const result = await response.json();
  if (!response.ok || result.status === 'failed') throw new Error(JSON.stringify(result));
  return result;
};
const clients = [];
async function connect(spec) {
  if (!config.browserTools) throw new Error('Native control requires the installed managed browser helper path');
  if(!Number.isInteger(spec.port)||spec.port<1024||spec.port>65535||typeof spec.target!=='string'||!Number.isInteger(spec.actor))throw new Error('Each native target requires an explicit local port, target id and durable actor id');
  const {managedCDP, scriptOwner} = await import(pathToFileURL(config.browserTools).href);
  let send;
  const call=await managedCDP(spec.port,spec.target,scriptOwner(),(method,params)=>send(method,params));
  const pages = await (await fetch(`http://127.0.0.1:${spec.port}/json/list`,{signal:AbortSignal.timeout(5000)})).json();
  const page = pages.find(p => p.id === spec.target);
  if (!page || new URL(page.url).origin !== origin.origin) throw new Error(`Native target ${spec.target} is missing or on the wrong origin`);
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve,reject) => {const timeout=setTimeout(()=>{ws.close();reject(new Error('CDP connection deadline'));},10000);ws.onopen=()=>{clearTimeout(timeout);resolve();};ws.onerror=e=>{clearTimeout(timeout);reject(e);};});
  const pending = new Map(); let serial = 0;
  ws.onmessage = event => {
    const row=JSON.parse(event.data), item=pending.get(row.id);
    if (!item) return;
    pending.delete(row.id);clearTimeout(item.timer);
    if (row.error) item.reject(new Error(JSON.stringify(row.error))); else item.resolve(row.result);
  };
  ws.onclose=()=>{for(const item of pending.values()){clearTimeout(item.timer);item.reject(new Error('CDP connection closed'));}pending.clear();};
  send = (method, params) => new Promise((resolve,reject) => {
    const n=++serial, timer=setTimeout(() => {pending.delete(n);reject(new Error(`CDP deadline: ${method}`));},10000);
    pending.set(n,{resolve,reject,timer}); ws.send(JSON.stringify({id:n,method,params}));
  });
  const evaluate=async expression => {
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    if(r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result.value;
  };
  const clock=async paused=>{
    const status=await evaluate(`(${nativeClock.toString()})(null)`);
    if(typeof status?.initialized!=='boolean')throw new Error('Native clock returned invalid initialization evidence');
    if(status.initialized)await evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))');
    return evaluate(`(${nativeClock.toString()})(${paused})`);
  };
  return {...spec,call,evaluate,clock,close:()=>ws.close()};
}
async function pauseAll() {
  const errors=[], confirmed=[];
  for (const c of clients) {
    try {
      await c.clock(true);
      confirmed.push(c.actor);
    } catch(e) {errors.push(String(e));}
  }
  let result;
  try {result=await api({op:'pause',native_paused:confirmed});} catch(e){errors.push(String(e));}
  if(errors.length) throw new Error(`Native pause unconfirmed; simulation stop requested: ${errors.join('; ')}`);
  return {...result,pause_scope:clients.length?'server_and_native':'server'};
}
async function resumeAll() {
  mayProgress=true;
  for (const c of clients) await c.clock(null);
  await api({op:'resume'});
  for (const c of clients) await c.clock(false);
}
const bindings = {
  w:['KeyW',87],a:['KeyA',65],s:['KeyS',83],d:['KeyD',68],
  ' ':['Space',32],Insert:['Insert',45],r:['KeyR',82],q:['KeyQ',81],e:['KeyE',69],
  '1':['Digit1',49],'2':['Digit2',50],'3':['Digit3',51],'4':['Digit4',52],
  '5':['Digit5',53],'6':['Digit6',54],'7':['Digit7',55],'8':['Digit8',56],
};
async function nativeAct(value) {
  const client=clients.find(c=>c.actor===value.actor);
  if(!client) throw new Error('Native act requires an explicitly configured owned native target for this actor');
  const input=value.input;
  if(!input||typeof input!=='object'||Object.keys(input).some(k=>!['duration_ms','keys','mouse'].includes(k)))throw new Error('Native input supports only duration_ms, keys and optional mouse');
  if(!Number.isInteger(input.duration_ms)||input.duration_ms<1||input.duration_ms>10000) throw new Error('Native duration_ms must be 1–10000');
  const keys=input.keys;
  if(!Array.isArray(keys)||keys.some(k=>!Object.hasOwn(bindings,k))) throw new Error('Native keys must be an explicit array of supported keys');
  if(input.mouse && (Object.keys(input.mouse).some(k=>!['x','y'].includes(k))||!Number.isFinite(input.mouse.x)||!Number.isFinite(input.mouse.y)||input.mouse.x<0||input.mouse.y<0||input.mouse.x>=800||input.mouse.y>=600)) throw new Error('Native mouse position must be inside the 800×600 client');
  const before=await api({op:'observe',actor:value.actor});
  if(before.controller!=='native'||!before.lifecycle.alive)throw new Error('Native input requires the same living actor with an active native controller');
  await client.call('Emulation.setFocusEmulationEnabled',{enabled:true});
  await client.evaluate(`document.querySelector('ruffle-player').focus({preventScroll:true})`);
  await resumeAll();
  const key=async(type,k)=>client.call('Input.dispatchKeyEvent',{type,key:k,code:bindings[k][0],windowsVirtualKeyCode:bindings[k][1]});
  let primary, cleanup=[];
  try {
    if(input.mouse) {
      const rect=await client.evaluate(`(()=>{const r=document.querySelector('ruffle-player').getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height,viewportWidth:innerWidth,viewportHeight:innerHeight}})()`);
      if(rect.width!==800||rect.height!==600||rect.x<0||rect.y<0||rect.x+800>rect.viewportWidth||rect.y+600>rect.viewportHeight) throw new Error('Native client is clipped or scaled; refusing uncertain input');
      await client.call('Input.dispatchMouseEvent',{type:'mouseMoved',x:rect.x+input.mouse.x,y:rect.y+input.mouse.y});
      await client.call('Input.dispatchMouseEvent',{type:'mousePressed',x:rect.x+input.mouse.x,y:rect.y+input.mouse.y,button:'left',buttons:1,clickCount:1});
    }
    for(const k of keys) await key('keyDown',k);
    await wait(input.duration_ms);
  } catch(e) {primary=e;}
  finally {
    for(const k of keys) try {await key('keyUp',k);} catch(e){cleanup.push(String(e));}
    if(input.mouse) try {
      const rect=await client.evaluate(`(()=>{const r=document.querySelector('ruffle-player').getBoundingClientRect();return{x:r.x,y:r.y}})()`);
      await client.call('Input.dispatchMouseEvent',{type:'mouseReleased',x:rect.x+input.mouse.x,y:rect.y+input.mouse.y,button:'left',buttons:0,clickCount:1});
    } catch(e){cleanup.push(String(e));}
  }
  try {await client.evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))');} catch(e){cleanup.push(String(e));}
  if(value.pauseAfter!==false) try {await pauseAll();} catch(e){cleanup.push(String(e));}
  if(primary||cleanup.length) throw new Error(JSON.stringify({primary:primary?String(primary):null,cleanup}));
  const after=await api({op:'observe',actor:value.actor});
  const effects={distance:Math.hypot(after.position.x-before.position.x,after.position.y-before.position.y),shots:after.verification.shots-before.verification.shots,hp:after.character.hp-before.character.hp,mp:after.character.mp-before.character.mp,inventory_changed:JSON.stringify(after.character.items)!==JSON.stringify(before.character.items),world_changed:after.session.world!==before.session.world};
  return {id,status:'input_complete',execution:'native',effects,paused:after.paused,pause_scope:value.pauseAfter===false?'unpaused':'server_and_native',native_clock:'pause_adjusted_page_time',actor:value.actor,state:after};
}
let result;
try {
  if(config.native!==undefined && !Array.isArray(config.native)) throw new Error('native must be an explicit array');
  const actors=new Set(),targets=new Set();
  for(const spec of config.native??[]) {
    const key=`${spec.port}/${spec.target}`;
    if(actors.has(spec.actor)||targets.has(key))throw new Error('Native actors and targets must each be unique');
    actors.add(spec.actor);targets.add(key);clients.push(await connect(spec));
  }
  const started=performance.now();
  if(command.op==='pause') result=await pauseAll();
  else if(command.op==='resume') {await resumeAll();result={paused:false};}
  else if(command.op==='act'&&command.input) result=await nativeAct(command);
  else if(['act','run'].includes(command.op)&&clients.length&&command.pauseAfter!==false) {
    await resumeAll();
    let primary;
    try {result=await api({...command,pauseAfter:false},id);} catch(e){primary=e;}
    let cleanup;
    try {await pauseAll();} catch(e){cleanup=e;}
    if(primary||cleanup) throw new Error(JSON.stringify({primary:primary?String(primary):null,pause:cleanup?String(cleanup):null}));
    const state=await api({op:'observe',actor:command.actor});
    if(result.result?.state)result.result.state=state;else if(result.result?.actor)result.result=state;
    result={...result,paused:true,pause_scope:'server_and_native'};
  } else result=await api(command,id);
  result={...result,client_wall_ms:Math.round(performance.now()-started)};
  await save(result);
  console.log(JSON.stringify(mutating?{...result,evidence:path}:result));
} catch(error) {
  let pauseError;
  if(mayProgress) try {await pauseAll();} catch(e){pauseError=String(e);}
  const failed={id,status:'failed',error:String(error),pause_error:pauseError};
  try {await save(failed);} catch(e){failed.evidence_error=String(e);}
  console.error(JSON.stringify(failed));
  process.exitCode=1;
} finally {for(const c of clients)c.close();}
