import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {input, point, socket, transfer, attachment, visible} from './record-demo.mjs';

const rect={x:8,y:114.25,width:800,height:600,pixels:[1200,900]};
test('visibility observes asynchronous restoration and refuses minimized or hidden pages',async()=>{
  let clock=0, polls=0;
  const c={call:async(method,p)=>{assert.equal(method,'Browser.getWindowForTarget');assert.equal(p.targetId,'owned');polls++;return {bounds:{windowState:polls===1?'minimized':'normal'}};},eval:async()=>polls<3};
  const options={now:()=>clock,wait:async ms=>{clock+=ms;}};
  assert.deepEqual(await visible(c,'owned',options),{windowState:'normal',hidden:false});
  assert.equal(polls,3);
  for (const state of [{windowState:'minimized',hidden:false},{windowState:'normal',hidden:true}]) {
    clock=0;const blocked={call:async()=>({bounds:{windowState:state.windowState}}),eval:async()=>state.hidden};
    await assert.rejects(visible(blocked,'owned',options),/visibility deadline/);
    assert.equal(clock,5000);
  }
});
test('normalized native input uses CSS geometry, not device pixels or guessed offsets',()=>{
  assert.deepEqual(point(rect,[.5,.5]),{x:408,y:414.25});
  assert.throws(()=>point(rect,[1,.5]),/normalized/);
  assert.throws(()=>point(rect,[NaN,.5]),/normalized/);
});
test('gesture queries geometry once and follows elapsed time without catch-up bursts',async()=>{
  let clock=0;const evaluations=[],moves=[];
  const c={eval:async e=>{evaluations.push(e);clock+=10;return rect;},call:async(method,p)=>{moves.push({time:clock,...p});clock+=2;}};
  const controls=input(c,{now:()=>clock,wait:async ms=>{clock+=ms;}});
  await controls.gesture([.2,.2],[.8,.8],600);
  assert.equal(evaluations.length,2);
  assert.ok(moves.length>=35 && moves.length<=38, String(moves.length));
  assert.deepEqual({x:moves[0].x,y:moves[0].y},point(rect,[.2,.2]));
  assert.ok(Math.abs(moves.at(-1).x-point(rect,[.8,.8]).x)<.001);
  assert.ok(moves.slice(1).every((m,i)=>m.time-moves[i].time>=16));
  await assert.rejects(controls.gesture([.2,.2],[.2,.2]),/Stationary/);
  await assert.rejects(controls.gesture([.2,.2],[.8,.8],Infinity),/bounded/);
});
test('slow dispatch skips missed slots rather than queueing an input burst',async()=>{
  let clock=0;const at=[];
  const c={eval:async()=>rect,call:async()=>{at.push(clock);clock+=40;}};
  await input(c,{now:()=>clock,wait:async ms=>{clock+=ms;}}).gesture([.2,.2],[.8,.8],600);
  assert.ok(at.slice(1).every((t,i)=>t-at[i]>=40));
});
test('key-up is attempted even when a scenario wait fails; released controllers reject input',async()=>{
  const calls=[];const c={call:async(m,p)=>calls.push(p),eval:async()=>rect};
  const controls=input(c,{wait:async()=>{throw Error('scenario failure');}});
  await assert.rejects(controls.key('w'),/scenario failure/);
  assert.deepEqual(calls.map(p=>p.type),['keyDown','keyUp']);
  await controls.release();await assert.rejects(controls.move([.5,.5]),/closed/);
});
test('release cancels a pending wait instead of keeping the process alive past its deadline',async()=>{
  const controls=input({call:async()=>{},eval:async()=>rect});
  const rejected=assert.rejects(controls.wait(100000),/aborted/);
  await controls.release();await rejected;
  assert.throws(()=>controls.wait(Infinity),/bounded/);
});
test('socket closure immediately rejects pending commands',async()=>{
  let instance;
  class FakeSocket {
    constructor(){instance=this;this.readyState=1;queueMicrotask(()=>this.onopen());}
    send(){} close(){this.readyState=3;this.onclose?.();}
  }
  const c=await socket('ws://local',FakeSocket);const result=c.call('Runtime.evaluate');
  instance.close();await assert.rejects(result,/socket closed/);c.close();
});
function recorded(data,{truncate=false,failure=null}={}) {
  const base64=data.toString('base64'),chunks=[];
  return {chunks,eval:async expression=>{
    if(expression.endsWith(')("stop",null)'))return {length:base64.length,bytes:data.length,failure,version:1,size:[1200,900]};
    const match=expression.match(/\)\("chunk",(\d+)\)$/);
    if(match){const offset=Number(match[1]);chunks.push(offset);const part=base64.slice(offset,offset+200000);return truncate?part.slice(1):part;}
    if(expression.endsWith(')("clear",null)'))return;
    throw Error('Unexpected recorder command');
  }};
}
test('bounded Blob export preserves exact bytes and rejects truncated replies',async()=>{
  const directory=await fs.mkdtemp(path.join(os.tmpdir(),'rotmg-chunks-'));
  try {
    const data=Buffer.alloc(350003,91),c=recorded(data);
    await transfer(c,directory,'good');
    assert.deepEqual(await fs.readFile(path.join(directory,'good.webm')),data);
    assert.deepEqual(c.chunks,[0,200000,400000]);
    await assert.rejects(transfer(recorded(data,{truncate:true}),directory,'truncated'),/Truncated/);
    assert.ok((await fs.stat(path.join(directory,'truncated.json'))).size>0);
    assert.ok((await fs.stat(path.join(directory,'truncated.webm.partial'))).isFile());
    await assert.rejects(transfer(recorded(Buffer.alloc(0)),directory,'empty'),/Empty/);
    await assert.rejects(transfer(recorded(data,{failure:'recorder failure'}),directory,'failed'),/Recorder failure/);
    assert.deepEqual(await fs.readFile(path.join(directory,'failed.webm')),data);
  } finally {await fs.rm(directory,{recursive:true});}
});
test('attachment byte mismatch cannot be mistaken for publication success',async()=>{
  const directory=await fs.mkdtemp(path.join(os.tmpdir(),'rotmg-attachment-')),originalFetch=globalThis.fetch;
  const file=path.join(directory,'demo.mp4');await fs.writeFile(file,'original');
  const url='https://github.com/user-attachments/assets/12345678-abcd';
  try {
    globalThis.fetch=async()=>new Response('different',{headers:{'content-type':'video/mp4'}});
    await assert.rejects(attachment(file,url),/bytes differ/);
    globalThis.fetch=async()=>new Response('original',{headers:{'content-type':'video/mp4'}});
    assert.equal((await attachment(file,url)).byteEqual,true);
    await assert.rejects(attachment(file,'https://example.org/demo.mp4'),/attachment URL/);
  } finally {globalThis.fetch=originalFetch;await fs.rm(directory,{recursive:true});}
});
