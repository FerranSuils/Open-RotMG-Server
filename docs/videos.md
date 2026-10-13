# 15-second gameplay demos

1. Before editing, capture the real failure/before case. Verify after through the same normal-client scenario. Keep captures and commit IDs locally, never in Git; no gameplay cheats or staged failures.
2. Record only the game canvas in an owned Chrome window: `canvas.captureStream(60)` + VP8 `MediaRecorder`, with browser-scoped input. Check frames arrive first; an occluded window can stop rendering. Open an owned window on the existing browser if needed. Never record the desktop or restart someone else's browser. Close the recording window afterward.
3. Measure actual source frames/timestamps with `ffprobe`; aim for ≥30 FPS. Screenshot polling and a 60 FPS export do not prove smooth capture. Retrieve large CDP Blobs in bounded chunks (200,000 base64 characters worked).
4. Edit: two-second **Open RotMG/date** overlay with fade/slide; about 1.25 seconds each of **Before/After**; then gameplay. Preserve 4:3 with blurred surroundings, not a caption sidebar. No explanatory paragraphs, development subtitles or “Play again.” Keep genuine client UI; choose clean shots without temporary debug labels. Claim only verified results.
5. Export silent H.264/yuv420p MP4, 1920×1080, 60 FPS, faststart, 15 seconds/900 frames, under 10 MB. No motion interpolation to disguise low source FPS. Watch framing and separately verify the gameplay outcome. Confirm metadata with `ffprobe` and decode fully with `ffmpeg -v error -i demo.mp4 -f null -`.

With publication approval, current `gh --attach` support and push access to the destination:

```sh
gh pr comment PR_NUMBER --repo OpenRotMG/Open-RotMG-Server --attach demo.mp4
```

Verify the published player. Attachments stay outside Git history; no releases/media branch needed. Fork-only contributors can attach in their own public fork and link its asset URL upstream, or ask a maintainer to attach. Never request upstream push access just for media. No privileged automatic upload workflow is configured.

https://docs.github.com/en/github-cli/github-cli/attaching-files-with-github-cli
