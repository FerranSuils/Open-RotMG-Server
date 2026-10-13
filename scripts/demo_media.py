import argparse
import datetime
import hashlib
import json
import math
import pathlib
import statistics
import struct
import subprocess

WIDTH, HEIGHT, FPS, SECONDS, MAX_BYTES = 1920, 1080, 60, 15, 10_000_000


def run(args):
    result = subprocess.run(args, capture_output=True, text=True, timeout=120)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed: {result.stderr.strip()}")
    return result.stdout


def probe(video, frames=False):
    args = ["ffprobe", "-v", "error", "-of", "json"]
    args += ["-show_frames", "-show_entries", "frame=best_effort_timestamp_time"] if frames else ["-count_frames", "-show_streams", "-show_format"]
    return json.loads(run(args + [str(video)]))


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values)*fraction)-1)]


def quality(times, meta, start, seconds, min_fps=30, min_cursor=30):
    if not all(math.isfinite(n) for n in (start,seconds,min_fps,min_cursor)) or start < 0 or seconds <= 0 or min_fps <= 0 or min_cursor <= 0:
        raise ValueError("Invalid quality interval or thresholds")
    if meta.get("version") != 1 or meta.get("failure") is not None:
        raise ValueError("Unsupported or failed recording sidecar")
    if any(not math.isfinite(t) or t < 0 for t in times):
        raise ValueError("Invalid source timestamp")
    previous = -math.inf
    for event in meta["inputs"]:
        if not math.isfinite(event["time"]) or event["time"] < previous:
            raise ValueError("Input timestamps regressed or are invalid")
        previous = event["time"]
        if event["type"].startswith("mouse") and any(not math.isfinite(event[n]) for n in ("x","y")):
            raise ValueError("Invalid received mouse position")
    selected = [t for t in times if start <= t < start+seconds]
    if len(selected) < 2:
        raise ValueError("No usable decoded source frames; check paint/occlusion")
    gaps = [b-a for a, b in zip(selected, selected[1:])]
    if any(g <= 0 for g in gaps):
        raise ValueError("Source frame timestamps are not strictly increasing")
    fps = (len(selected)-1)/(selected[-1]-selected[0])
    cursor_gaps, errors = [], []
    for gesture in meta["gestures"]:
        if gesture["end"] is None:
            raise ValueError("Incomplete input gesture")
        if not all(math.isfinite(gesture[n]) for n in ("start","end")) or gesture["end"] < gesture["start"]:
            raise ValueError("Invalid gesture interval")
        overlap = min(gesture["end"],(start+seconds)*1000)-max(gesture["start"],start*1000)
        if overlap < 200:
            continue
        events = [e for e in meta["inputs"] if e["type"] == "mousemove" and e.get("gesture") == gesture["id"] and start*1000 <= e["time"] < (start+seconds)*1000]
        if len(events) < 3:
            errors.append("Gesture has insufficient received mouse events")
        for a, b in zip(events, events[1:]):
            if (a["x"], a["y"]) != (b["x"], b["y"]):
                gap = b["time"]-a["time"]
                if gap <= 0:
                    raise ValueError("Input timestamps regressed")
                cursor_gaps.append(gap)
    # WebM millisecond timestamps put an exact 30 FPS stream fractionally below 30.
    if round(fps,2) < min_fps:
        errors.append(f"Moving-source target missed: {fps:.2f} FPS < {min_fps}")
    if max(gaps) > .2:
        errors.append(f"Source stalled for {max(gaps):.3f}s")
    cursor = {"movingIntervals": len(cursor_gaps), "measured":bool(cursor_gaps), "scope": "Actual received events in explicit gestures; stationary holds excluded"}
    if cursor_gaps:
        cursor.update(medianMs=statistics.median(cursor_gaps), p95Ms=percentile(cursor_gaps,.95), maxMs=max(cursor_gaps))
        cursor["medianHz"] = 1000/cursor["medianMs"]
        if cursor["medianHz"] < min_cursor or cursor["p95Ms"] > 2000/min_cursor or cursor["maxMs"] > 200:
            errors.append(f"Cursor cadence missed: {cursor['medianHz']:.2f} median Hz, {cursor['p95Ms']:.1f}ms p95, {cursor['maxMs']:.1f}ms max")
    return {"qualified": not errors, "errors": errors, "source": {"frames": len(selected), "decodedFps": fps, "maxGapMs": max(gaps)*1000, "p95GapMs": percentile(gaps,.95)*1000, "interval": [start,start+seconds]}, "cursor": cursor}


def check(source, sidecar, start, seconds, min_fps=30, min_cursor=30):
    meta = json.loads(pathlib.Path(sidecar).read_text())
    streams = probe(source)["streams"]
    if len(streams) != 1 or streams[0]["codec_type"] != "video" or streams[0]["codec_name"] != "vp8":
        raise ValueError("Expected one original VP8 canvas track, no audio")
    size = [streams[0]["width"],streams[0]["height"]]
    if size != meta["size"] or size[0]*3 != size[1]*4:
        raise ValueError("Source/sidecar dimensions disagree or are not 4:3")
    times = [float(f["best_effort_timestamp_time"]) for f in probe(source,True)["frames"]]
    if start < 0 or seconds <= 0 or start+seconds > meta["duration"]+.05:
        raise ValueError("Selected interval lies outside the recording")
    return quality(times,meta,start,seconds,min_fps,min_cursor)


def verify(video):
    video = pathlib.Path(video)
    info = probe(video)
    streams = info["streams"]
    if len(streams) != 1:
        raise ValueError("Export must contain one video track and no audio")
    stream = streams[0]
    expected = {"codec_name":"h264", "pix_fmt":"yuv420p", "width":WIDTH, "height":HEIGHT, "r_frame_rate":f"{FPS}/1", "nb_read_frames":str(FPS*SECONDS)}
    for key, value in expected.items():
        if stream.get(key) != value:
            raise ValueError(f"Invalid export {key}: {stream.get(key)!r}, expected {value!r}")
    if abs(float(info["format"]["duration"])-SECONDS) > .001 or video.stat().st_size >= MAX_BYTES:
        raise ValueError("Export must be 15 seconds and under 10 MB")
    boxes = []
    with video.open("rb") as file:
        while file.tell() < video.stat().st_size:
            header = file.read(8)
            if len(header) != 8:
                raise ValueError("Truncated MP4 box")
            size, kind = struct.unpack(">I4s", header)
            if size < 8:
                raise ValueError("Unsupported or invalid MP4 box size")
            boxes.append(kind); file.seek(size-8,1)
    if b"moov" not in boxes or b"mdat" not in boxes or boxes.index(b"moov") > boxes.index(b"mdat"):
        raise ValueError("Export lacks faststart")
    run(["ffmpeg","-v","error","-threads","2","-i",str(video),"-f","null","-"])
    return {"bytes":video.stat().st_size, "sha256":hashlib.sha256(video.read_bytes()).hexdigest(), "duration":SECONDS, "frames":FPS*SECONDS, "size":[WIDTH,HEIGHT], "fps":FPS, "fullDecode":True, "faststart":True, "silent":True}


def render(take, start, date, before=None, after=None):
    from PIL import Image, ImageDraw, ImageFont, ImageFilter, ImageEnhance
    directory = pathlib.Path(take)
    source, sidecar = directory/"source.webm", directory/"source.json"
    report = check(source,sidecar,start,SECONDS)
    (directory/"quality.json").write_text(json.dumps(report,indent=2))
    if not report["qualified"]:
        raise ValueError("Unqualified take: "+"; ".join(report["errors"]))
    meta = json.loads(sidecar.read_text()); width,height = meta["size"]
    for value in (before,after):
        if value is not None and not 2 <= value <= SECONDS-1.25:
            raise ValueError("Labels must fit after the opening title, inside the selected take")
    date = datetime.date.fromisoformat(date).strftime("%B %d, %Y")
    fonts = {n:ImageFont.truetype("NotoSans-Bold.ttf",n) for n in (52,90)}
    regular = ImageFont.truetype("NotoSans-Regular.ttf",34)
    final, partial = directory/"demo.mp4", directory/"demo.mp4.partial"
    if final.exists() or partial.exists():
        raise ValueError("Export already exists; preserve it and use a new take")
    def envelope(t,begin,end):
        return max(0,min(1,(t-begin)/.25,(end-t)/.25))
    def title(im,text,y,font,alpha):
        layer = Image.new("RGBA",im.size); draw = ImageDraw.Draw(layer)
        box = draw.textbbox((0,0),text,font=font)
        draw.text(((WIDTH-box[2])//2,y),text,font=font,fill=(255,255,255,int(255*alpha)),stroke_width=4,stroke_fill=(0,0,0,int(200*alpha)))
        return Image.alpha_composite(im.convert("RGBA"),layer).convert("RGB")
    error_log = directory/"encode.log"
    with error_log.open("w") as log:
        dec = subprocess.Popen(["ffmpeg","-v","error","-threads","2","-ss",str(start),"-i",str(source),"-t",str(SECONDS),"-vf",f"fps={FPS}","-pix_fmt","rgb24","-f","rawvideo","pipe:1"],stdout=subprocess.PIPE,stderr=log)
        enc = subprocess.Popen(["ffmpeg","-v","error","-n","-f","rawvideo","-pixel_format","rgb24","-video_size",f"{WIDTH}x{HEIGHT}","-framerate",str(FPS),"-i","pipe:0","-an","-c:v","libx264","-preset","medium","-crf","23","-threads","2","-pix_fmt","yuv420p","-movflags","+faststart","-f","mp4",str(partial)],stdin=subprocess.PIPE,stderr=log)
        thumbs, index, current = [], 0, None
        try:
            for n in range(FPS*SECONDS):
                t = n/FPS; raw = dec.stdout.read(width*height*3)
                if len(raw) != width*height*3:
                    raise ValueError("Continuous source ended before 900 export frames")
                src = Image.frombytes("RGB",(width,height),raw)
                bg = src.resize((256,144),Image.Resampling.BILINEAR).filter(ImageFilter.GaussianBlur(7)).resize((WIDTH,HEIGHT),Image.Resampling.BILINEAR)
                im = ImageEnhance.Brightness(bg).enhance(.45)
                im.paste(src.resize((1440,1080),Image.Resampling.NEAREST),(240,0))
                if t < 2:
                    alpha = envelope(t,0,2); im = ImageEnhance.Brightness(im).enhance(1-.35*alpha)
                    im = title(im,"Open RotMG",390+int(30*(1-alpha)),fonts[90],alpha)
                    im = title(im,date,520,regular,alpha)
                for label,begin in (("Before",before),("After",after)):
                    if begin is not None:
                        im = title(im,label,940,fonts[52],envelope(t,begin,begin+1.25))
                while index < len(meta["inputs"]) and meta["inputs"][index]["time"]/1000 <= t+start:
                    event = meta["inputs"][index]; index += 1
                    if event["type"].startswith("mouse"):
                        current = event
                if current and current["inside"]:
                    # Holding received positions preserves real input timing; interpolation would invent motion.
                    x,y = 240+current["x"]*1440/width, current["y"]*1080/height
                    ImageDraw.Draw(im).polygon([(x,y),(x+2,y+34),(x+11,y+25),(x+18,y+38),(x+25,y+34),(x+18,y+22),(x+33,y+22)],fill="white",outline="black",width=3)
                if t > SECONDS-.2:
                    im = ImageEnhance.Brightness(im).enhance((SECONDS-t)/.2)
                if n % 90 == 45:
                    thumbs.append(im.resize((480,270)))
                enc.stdin.write(im.tobytes())
            dec.stdout.read(); dec.stdout.close(); enc.stdin.close()
            if dec.wait() or enc.wait():
                raise ValueError(f"Encoding failed; inspect {error_log}")
        finally:
            for process in (dec,enc):
                if process.poll() is None:
                    process.terminate(); process.wait(timeout=5)
    exported = verify(partial); partial.rename(final)
    sheet = Image.new("RGB",(2400,540),"#171717")
    for i,im in enumerate(thumbs):
        sheet.paste(im,((i%5)*480,(i//5)*270))
    sheet.save(directory/"contact-sheet.jpg",quality=92)
    receipt = {"sourceQuality":report,"export":exported,"scope":"Technical recording checks only; watch the interaction and verify gameplay outcomes separately."}
    (directory/"export.json").write_text(json.dumps(receipt,indent=2))
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command",required=True)
    checker = commands.add_parser("check")
    for name in ("source","meta","start","seconds","report"):
        checker.add_argument("--"+name,required=True,type=float if name in ("start","seconds") else str)
    exporter = commands.add_parser("render")
    for name in ("take","start","date"):
        exporter.add_argument("--"+name,required=True,type=float if name == "start" else str)
    exporter.add_argument("--before",type=float); exporter.add_argument("--after",type=float)
    validator = commands.add_parser("verify"); validator.add_argument("--video",required=True)
    args = parser.parse_args()
    if args.command == "check":
        result = check(args.source,args.meta,args.start,args.seconds)
        pathlib.Path(args.report).write_text(json.dumps(result,indent=2))
        print(json.dumps(result,indent=2))
        if not result["qualified"]:
            raise SystemExit(1)
    else:
        result = render(args.take,args.start,args.date,args.before,args.after) if args.command == "render" else verify(args.video)
        print(json.dumps(result,indent=2))
