import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,appendFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import vm from 'node:vm';
import {exampleNexusReturn} from '../examples/testing/ui-transition.mjs';

const entry = world => JSON.stringify({fields:{message:'player entered development world',character:1,world}})+'\n';
async function fixture(work) {
  const dir=await mkdtemp(join(tmpdir(),'rotmg-focus-test-')),log=join(dir,'server.log');
  await writeFile(log,entry('Tutorial'));
  const calls=[];
  const call=async(method,params)=>{
    calls.push({method,params});
    if(method==='Runtime.evaluate')return {result:{value:JSON.stringify({x:0,y:0,width:800,height:600,scale:1})}};
    if(method==='Input.dispatchKeyEvent'&&params.type==='keyDown')await appendFile(log,entry('Nexus'));
    return {};
  };
  try {await work({log,calls,call});} finally {await rm(dir,{recursive:true});}
}
test('Nexus observation uses page-local focus and restores it without desktop activation',async()=>{
  await fixture(async({log,calls,call})=>{
    const result=await exampleNexusReturn(call,log,1);
    assert.deepEqual({world:result.world,focus:calls.filter(c=>c.method==='Emulation.setFocusEmulationEnabled').map(c=>c.params.enabled),activation:calls.filter(c=>/bringToFront|activateTarget|setWindowBounds/.test(c.method))},{world:'Nexus',focus:[true,false],activation:[]});
  });
});
test('DOM focus is local to the Ruffle element and does not scroll the page',async()=>{
  await fixture(async({log,call})=>{
    const document={hidden:false,hasFocus:()=>true};let options;
    const player={focus:value=>{options=value;document.activeElement=player;},getBoundingClientRect:()=>({x:0,y:0,width:800,height:600,left:0,top:0,right:800,bottom:600})};
    document.querySelector=()=>player;
    await exampleNexusReturn(async(method,params)=>method==='Runtime.evaluate'?{result:{value:vm.runInNewContext(params.expression,{document,innerWidth:800,innerHeight:600})}}:call(method,params),log,1);
    assert.equal(options.preventScroll,true);
  });
});
test('failed focus guard dispatches no key-down and still disables focus emulation',async()=>{
  await fixture(async({log,calls,call})=>{
    let caught;
    try {await exampleNexusReturn(async(m,p)=>m==='Runtime.evaluate'?{exceptionDetails:{text:'focus unavailable'}}:call(m,p),log,1);}catch(error){caught=error;}
    assert.deepEqual({error:caught?.message,keyDown:calls.some(c=>c.params.type==='keyDown'),released:calls.at(-1)},{error:'{"text":"focus unavailable"}',keyDown:false,released:{method:'Emulation.setFocusEmulationEnabled',params:{enabled:false}}});
  });
});
test('cleanup errors cannot hide the original dispatch error or skip focus release',async()=>{
  await fixture(async({log,calls,call})=>{
    const original=console.error;console.error=()=>{};
    try {
      let thrown;
      try {await exampleNexusReturn(async(m,p)=>{
        calls.push({method:m,params:p});
        if(m==='Input.dispatchKeyEvent')throw Error(p.type==='keyDown'?'dispatch failed':'release failed');
        return m==='Runtime.evaluate'?{result:{value:JSON.stringify({x:0,y:0,width:800,height:600,scale:1})}}:{};
      },log,1);}catch(error){thrown=error;}
      assert.deepEqual({error:thrown?.message,last:calls.at(-1)},{error:'dispatch failed',last:{method:'Emulation.setFocusEmulationEnabled',params:{enabled:false}}});
    }finally{console.error=original;}
  });
});
test('wrong destination and rotated logs are rejected instead of reported as Nexus',async()=>{
  await fixture(async({log,call})=>{
    await assert.rejects(exampleNexusReturn(async(m,p)=>{if(m==='Input.dispatchKeyEvent'&&p.type==='keyDown'){await appendFile(log,entry('Realm'));return {};}return call(m,p);},log,1),/wrong world/);
  });
  await fixture(async({log,call})=>{
    await assert.rejects(exampleNexusReturn(async(m,p)=>{if(m==='Input.dispatchKeyEvent'&&p.type==='keyDown'){await writeFile(log,entry('Nexus'));return {};}return call(m,p);},log,1),/rotated/);
  });
});
