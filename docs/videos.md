# Gameplay demos

Make a 15-second clip for gameplay/visual PRs. Keep captures, edits and exports outside Git. Record the broken/before case before changing code, using the same scenario for after. Keep the source captures and commit IDs locally so the comparison can be reproduced.

## Capture

1. Run the sibling client/server using the README. Use a fresh, agent-owned browser context and normal guest gameplay. Choose a short acceptance scenario showing the change; no teleports, edited health or fake failures.
2. Record only the game canvas from a visible, agent-owned Chrome window using `canvas.captureStream(60)` and `MediaRecorder` (`video/webm;codecs=vp8`). Exercise the scenario with browser-scoped mouse/keyboard tools. Polling screenshots produced choppy footage and is not video capture.
3. Confirm real frames arrive before performing the scenario. An occluded window can report visible/focused yet produce no animation frames. Open an owned window on the existing browser if necessary; do not restart another person's browser or alter security flags. Stop the recorder and close the owned recording window afterward.
4. Save the recorded Blob. When retrieving it through CDP, transfer base64 in bounded chunks (200,000 characters worked); large replies can close the connection. Measure source frame count and timestamps with `ffprobe`, aiming for at least 30 actual FPS. Export FPS alone is not evidence of smooth capture.

## Edit and verify

- Use 1920×1080 landscape. Preserve the 4:3 game view, with blurred footage filling the surrounding space rather than a caption sidebar.
- Opening: about two seconds of **Open RotMG** and the recording date over gameplay, with a short fade/slide. Then about 1.25 seconds each of **Before** and **After**, followed by the demonstrated gameplay. Use brief crossfades between cuts.
- Add no explanatory paragraphs, tutorial/development subtitles or “Play again” captions. Let the PR explain the accomplishment. Leave genuine client UI intact; use clean shots rather than temporary debug labels. Do not imply historical fidelity or systems that the scenario does not verify.
- Export H.264, `yuv420p`, 60 FPS and MP4 faststart. Do not interpolate motion to conceal low source FPS. The approved draft was silent. Keep the MP4 below 10 MB for free-plan GitHub attachments.
- Watch the final clip, check framing/readability/privacy and verify the actual gameplay outcome separately. Run:

```sh
ffprobe -v error -show_entries format=duration,size:stream=codec_name,width,height,avg_frame_rate,nb_frames -of json demo.mp4
ffmpeg -v error -i demo.mp4 -f null -
```

Expected export: 15 seconds, 1920×1080, H.264, 60 FPS/900 frames, and no decode errors. Keep keys, tokens, databases and private evidence out of the clip and PR.

## Attach to the PR

With a current `gh` supporting `--attach` and push access to the destination repository:

```sh
gh pr comment PR_NUMBER --repo OpenRotMG/Open-RotMG-Server --attach demo.mp4
```

This uploads an attachment and embeds a player in the comment. It does not put the video in Git history; no release or media branch is needed. Check the published player and that the attached file is the final export. Obtain publication approval when it has not already been authorized.

Fork-only contributors cannot assume upstream upload permission. They can upload an attachment in their own public fork and put its public asset URL in the upstream PR, or ask a maintainer to attach the locally produced clip. Do not request upstream push permission just to upload a demo. No automatic privileged upload workflow is configured here.

https://docs.github.com/en/github-cli/github-cli/attaching-files-with-github-cli
