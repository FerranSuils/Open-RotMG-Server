# Four testing recipes

Copy and adapt these into scratch work. They are examples, not a scenario framework; adding shared helpers or more examples requires explicit approval. Prepared state never proves earned progression. Keep formatting, focused tests and long native journeys separate.

- **Cover:** `cargo test --locked example_cover` constructs a hostile shot behind cover using real collision code.
- **Timing:** `cargo test --locked example_timing` sends SHOOT before MOVE. For sustained-fire and skipped-clock variants, see the existing native attack regressions.
- **Prepared state:** capture the guest GUID from your own disposable QA browser's character-list request. Run `ROTMG_QA_GUID=… cargo test --locked example_prepared_state -- --ignored --nocapture`. It creates a new private temporary directory, a level-eight Wizard and prepared equipment through the current Store API. Start `./run.sh PORT --state PRINTED_DIRECTORY --client CLIENT_DIRECTORY`, then LOAD that character in the same QA profile. Never use a production identity/profile. The server has no new seeding endpoint.
- **UI transition:** import `exampleNexusReturn` from `ui-transition.mjs`; pass your owned tab's CDP call function, its server log path and character ID. Start outside Nexus after the game HUD has rendered. The recipe uses page-local focus emulation and releases it afterward; it never activates a desktop window. It sends the selected client's Insert binding and requires a NEW same-character Nexus entry. Key-release failure cannot hide the original error. This demonstrates observation, not the boss portal's UI selection.

On KDE/Wayland, start a private GPU-backed virtual compositor in its own session bus; never attach it to host KWin. Use a fresh absolute `QA_DISPLAY` directory and distinct port/profile:

```sh
mkdir -m 700 "$QA_DISPLAY" "$QA_DISPLAY/config" "$QA_DISPLAY/data"
env -u DISPLAY -u WAYLAND_DISPLAY XDG_RUNTIME_DIR="$QA_DISPLAY" \
  XDG_CONFIG_HOME="$QA_DISPLAY/config" XDG_DATA_HOME="$QA_DISPLAY/data" \
  QT_QPA_PLATFORM=offscreen dbus-run-session -- kwin_wayland --virtual \
  --socket rotmg-qa --width 1400 --height 1000 --no-global-shortcuts \
  --no-kactivities --no-lockscreen
```

In another terminal, retain the normal user bus/runtime for the managed launcher and Chrome's keyring, but point Chrome exclusively at that private display:

```sh
env -u DISPLAY WAYLAND_DISPLAY="$QA_DISPLAY/rotmg-qa" \
  CHROME_MCP_CPUQUOTA=400% node --input-type=module - <<'JS'
import {launchBrowser} from '/home/wacket/.local/share/pi-browser-tools/launch-browser.mjs';
console.log(await launchBrowser({owner:process.env.PI_BROWSER_OWNER,args:[
  `--remote-debugging-port=${process.env.CHROME_PORT}`,
  `--user-data-dir=${process.env.CHROME_PROFILE}`, '--ozone-platform=wayland',
  '--window-size=1280,960', '--no-first-run', '--no-default-browser-check', 'about:blank',
]}));
JS
```

Export the port/profile variables; jobs outside Pi must supply an explicit `PI_BROWSER_OWNER`. The 400% ceiling applies only to this QA browser; keep the launcher's low CPU weight and shared defaults unchanged. The default 150% scope throttled renderer/GPU/recording children here even though Chrome's parent had moved to an uncapped desktop-app scope. Inspect the actual renderer/GPU child cgroup, not only the parent. Hardware acceleration also needs evidence: Chrome `SystemInfo.getInfo` and private KWin `supportInformation` identified AMD hardware rendering and a 60 Hz virtual output; merely opening a render node is not proof. This corrects the measured quota bottleneck, not every source of lag: one 17-second native take qualified at40.8 source FPS/45Hz received cursor movement, but fresh retakes still failed during high shared CPU pressure despite zero quota throttling. Keep the quality gates; do not raise priority above desktop applications, stop other owners' work or claim this ceiling guarantees capture cadence. Use managed target creation/activity helpers. Close owned targets/Chrome before stopping the owned compositor; preserve profiles. Do not use host `Page.bringToFront`, `Target.activateTarget` or MCP `bringToFront:true`. Focus emulation alone does not guarantee render cadence: verify actual frames and native outcomes, without host activation as repair. Xvfb alone failed native rendering here (WebGL blocklisting/AVM2 errors); do not silently substitute it or relax GPU/security flags.

Rust recipes compile with the existing tests; the prepared-state recipe runs only when explicitly selected. No browser dependency is bundled: use your existing browser-scoped connection, never desktop input. Keep generated state, media and exploratory scripts outside Git.
