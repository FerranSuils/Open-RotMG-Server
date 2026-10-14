import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {nativeClock} from './native-clock.mjs';

function fixture(playing=true) {
  let wall=100;
  class WallDate extends Date {static now(){return 1000000+wall;}}
  const api={isPlaying:playing,suspend(){this.isPlaying=false;},resume(){this.isPlaying=true;}};
  const context=vm.createContext({Date:WallDate,performance:{now:()=>wall},document:{querySelector:()=>({ruffle:()=>api})}});
  const call=arg=>JSON.parse(JSON.stringify(vm.runInContext(`(${nativeClock.toString()})(${JSON.stringify(arg)})`,context)));
  const clocks=()=>JSON.parse(vm.runInContext('JSON.stringify([performance.now(),Date.now(),new Date().getTime()])',context));
  return {call,clocks,api,advance:ms=>{wall+=ms;},context};
}
test('twenty seconds paused funds neither performance nor Date clocks on resume',()=>{
  const f=fixture();f.call(null);const before=f.clocks();f.call(true);f.advance(20000);
  const paused=f.clocks();f.call(false);const resumed=f.clocks();f.advance(40);
  assert.deepEqual([before,paused,resumed,f.clocks()],[[100,1000100,1000100],[100,1000100,1000100],[100,1000100,1000100],[140,1000140,1000140]]);
});
test('Date.now preserves integer milliseconds during active page time',()=>{
  const f=fixture();f.call(null);f.advance(0.25);assert.deepEqual(f.clocks(),[100.25,1000100,1000100]);
});
test('repeated pauses do not double subtract and active time remains real time',()=>{
  const f=fixture();f.call(true);f.advance(20);f.call(true);f.advance(30);f.call(false);f.advance(60);f.call(false);
  assert.deepEqual(f.clocks(),[160,1000160,1000160]);
});
test('untracked prior native suspension fails closed before clock installation',()=>{
  const f=fixture(false);assert.throws(()=>f.call(null),/Untracked native pause/);
});
test('out of band native playback cannot silently resume a stopped clock',()=>{
  const f=fixture();f.call(true);f.api.resume();assert.throws(()=>f.call(false),/outside the coordinated bridge/);
});
test('explicit Date construction, static methods and callable Date remain compatible',()=>{
  const f=fixture();f.call(null);
  assert.deepEqual(JSON.parse(vm.runInContext('JSON.stringify([new Date(0).getTime(),Date.UTC(1970,0,1),Date.parse("1970-01-01T00:00:00Z"),typeof Date()])',f.context)),[0,0,0,'string']);
});
