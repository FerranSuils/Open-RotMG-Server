import { readFile } from 'node:fs/promises';
import { setTimeout as wait } from 'node:timers/promises';

// Supply the CDP call function for your owned QA tab; this example never launches or adopts a browser.
export async function exampleNexusReturn(call, serverLog, character) {
  const log = await readFile(serverLog, 'utf8');
  const before = log.slice(0, log.lastIndexOf('\n') + 1);
  // The logger may be halfway through its next write; only complete lines are evidence.
  const entries = text => text.split('\n').slice(0, -1).filter(line => line.startsWith('{'))
    .map(line => JSON.parse(line))
    .filter(row => row.fields?.message === 'player entered development world'
      && row.fields.character === character);
  const initial = entries(before).at(-1);
  if (!initial || typeof initial.fields.world !== 'string' || initial.fields.world === 'Nexus') {
    throw new Error('Start this example with the owned character in a non-Nexus world');
  }
  const bounds = await call('Runtime.evaluate', {
    expression: `JSON.stringify((() => {
      const player = document.querySelector('ruffle-player');
      if (!player) throw Error('Ruffle game not found');
      if (document.hidden || !document.hasFocus() || document.activeElement !== player)
        throw Error('Focus the owned Ruffle game before dispatching input');
      const r = player.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0 || r.left < 0 || r.top < 0 || r.right > innerWidth || r.bottom > innerHeight)
        throw Error('Game lies outside input viewport');
      return {x:r.x, y:r.y, width:r.width, height:r.height, scale:1};
    })())`, returnByValue: true,
  });
  if (bounds.exceptionDetails) throw new Error(JSON.stringify(bounds.exceptionDetails));
  const clip = JSON.parse(bounds.result.value);
  const key = { key: 'Insert', code: 'Insert', windowsVirtualKeyCode: 45 };
  let failure;
  try {
    await call('Input.dispatchKeyEvent', { type: 'keyDown', ...key });
    await call('Page.captureScreenshot', {format:'png', clip});
    await wait(80);
    await call('Input.dispatchKeyEvent', { type: 'keyUp', ...key });
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      // Reading the actual game frame also keeps hidden QA windows advancing.
      await call('Page.captureScreenshot', {format:'png', clip});
      const current = await readFile(serverLog, 'utf8');
      if (!current.startsWith(before)) throw new Error('Server log rotated during the transition');
      const entered = entries(current.slice(before.length)).at(-1);
      if (entered) {
        if (entered.fields.world !== 'Nexus') throw new Error('Character entered the wrong world');
        console.log(JSON.stringify(entered.fields));
        return entered.fields;
      }
      await wait(100);
    }
    throw new Error(`Insert dispatched for character ${character} in ${initial.fields.world}, but no new Nexus entry was observed`);
  } catch (error) {
    failure = error;
    throw error;
  } finally {
    try {
      await call('Input.dispatchKeyEvent', { type: 'keyUp', ...key });
    } catch (cleanup) {
      if (!failure) throw cleanup;
      console.error('Key release also failed; original transition error retained:', cleanup);
    }
  }
}
