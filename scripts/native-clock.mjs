export function nativeClock(paused) {
  const player=document.querySelector('ruffle-player');
  if(!player)throw new Error('Ruffle missing');
  const api=player.ruffle(), key=Symbol.for('openrotmg.testing.native-clock.v1');
  let clock=globalThis[key];
  if(!clock) {
    if(!api.isPlaying){globalThis[key]={invalid:true};throw new Error('Untracked native pause: reconnect the same living character before initializing the native clock');}
    const raw=performance.now.bind(performance), NativeDate=Date, epoch=NativeDate.now()-raw();
    let stopped=null, excluded=0;
    const now=()=>(stopped===null?raw():stopped)-excluded;
    // Ruffle suspend stops frames, but AVM getTimer still reads performance.now.
    Object.defineProperty(performance,'now',{value:now,configurable:true});
    function PausableDate(...args) {
      if(!new.target)return new NativeDate(epoch+now()).toString();
      return Reflect.construct(NativeDate,args.length?args:[epoch+now()],new.target);
    }
    Object.setPrototypeOf(PausableDate,NativeDate);PausableDate.prototype=NativeDate.prototype;
    Object.defineProperty(PausableDate,'now',{value:()=>Math.trunc(epoch+now())});globalThis.Date=PausableDate;
    clock=globalThis[key]={pause:()=>{if(stopped===null)stopped=raw();},resume:()=>{if(stopped!==null){excluded+=raw()-stopped;stopped=null;}},read:()=>({paused:stopped!==null,clock_ms:now()})};
  }
  if(clock.invalid||clock.read().paused===api.isPlaying) {
    clock.invalid=true;
    if(paused===true){api.suspend();if(clock.pause)clock.pause();}
    throw new Error('Native clock invalid after untracked pause or playback change; reconnect the same living character');
  }
  if(paused===true) {
    api.suspend();clock.pause();
    if(api.isPlaying)throw new Error('Ruffle did not pause');
  } else if(paused===false){clock.resume();api.resume();if(!api.isPlaying)throw new Error('Ruffle did not resume');}
  return clock.read();
}
