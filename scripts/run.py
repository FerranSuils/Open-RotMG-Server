#!/usr/bin/env python3
import argparse
import hashlib
import io
from pathlib import Path
import shutil
import signal
import subprocess
import sys
from urllib.request import urlopen
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]
REVISION = "edfb8a4b876bc66211ca29d5a55f2a56619bd024"
URL = f"https://codeload.github.com/OpenRotMG/Open-RotMG-Client/zip/{REVISION}"
SHA256 = "420143cf3abc5d989cd08d44dd8b764009e827bc0db16c0e9aecaf6061d5ccad"
LOG = ROOT / ".local/server.log"


def prepare_client():
    cache = ROOT / f".local/client-{REVISION}.zip"
    cache.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if cache.exists():
        data = cache.read_bytes()
    else:
        print(f"Downloading local client: {URL}", flush=True)
        try:
            with urlopen(URL, timeout=60) as response:
                data = response.read()
        except Exception as error:
            raise RuntimeError(f"Download {URL} failed: {type(error).__name__}: {error}") from error
    actual = hashlib.sha256(data).hexdigest()
    if actual != SHA256:
        raise RuntimeError(f"Client archive checksum mismatch: expected {SHA256}, got {actual}. Remove {cache} and rerun.")
    prefix = f"Open-RotMG-Client-{REVISION}"
    client = cache.parent / prefix
    with ZipFile(io.BytesIO(data)) as archive:
        for item in archive.infolist():
            path = Path(item.filename)
            if path.is_absolute() or ".." in path.parts or path.parts[0] != prefix or (item.external_attr >> 16) & 0o170000 not in (0, 0o100000, 0o040000):
                raise RuntimeError(f"Unsafe client archive entry: {item.filename}")
        if not client.exists():
            stage = cache.parent / f"client-extract-{REVISION}"
            stage.mkdir(mode=0o700)
            archive.extractall(stage)
            (stage / prefix).rename(client)
            stage.rmdir()
        for name in ["client.swf", "index.html", "scripts/run.py"]:
            path = client / name
            if not path.is_file() or path.read_bytes() != archive.read(f"{prefix}/{name}"):
                raise RuntimeError(f"Cached client asset is missing or modified: {path}. Remove {client} and rerun.")
    if not cache.exists():
        cache.write_bytes(data)
    return client


def run(command, cwd, log):
    with subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) as process:
        try:
            for line in process.stdout:
                print(line, end="", flush=True)
                log.write(line)
                log.flush()
            return process.wait()
        except KeyboardInterrupt:
            process.send_signal(signal.SIGINT)
            process.wait()
            raise


def port_number(value):
    try:
        port = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("port must be an integer between 1 and 65535") from None
    if not 1 <= port <= 65535:
        raise argparse.ArgumentTypeError("port must be between 1 and 65535")
    return port


def main():
    parser = argparse.ArgumentParser(description="Download the client and start your local OpenRotMG game.")
    parser.add_argument("port", nargs="?", type=port_number, help="Local server port (default: 18923)")
    parser.add_argument("--local-client", type=Path, metavar="DIR", help="Use your own client checkout instead of downloading the pinned client")
    parser.add_argument("--dev-control", action="store_true", help="Enable loopback-only QA control")
    args = parser.parse_args()
    if shutil.which("cargo") is None:
        parser.error("Cargo not found. Install Rust/Cargo from https://rustup.rs/ and rerun.")
    ROOT.joinpath(".local").mkdir(mode=0o700, exist_ok=True)
    with LOG.open("a") as log:
        try:
            client = args.local_client.resolve() if args.local_client is not None else prepare_client()
            prepare = client / "scripts/run.py"
            if not prepare.is_file():
                raise RuntimeError(f"Missing client launcher: {prepare}. Use a complete Open-RotMG-Client checkout.")
            print(f"Preparing local client. Logs: {LOG}", flush=True)
            code = run([sys.executable, str(prepare), "--prepare-only"], client, log)
            if code:
                raise RuntimeError(f"Client preparation exited with status {code}. See the error above and {LOG}.")
            print(f"Building and starting your local game. Logs: {LOG}", flush=True)
            command = ["cargo", "run", "--locked", "--", "--client", str(client)]
            if args.port is not None:
                command.extend(["--port", str(args.port)])
            if args.dev_control:
                command.append("--dev-control")
            code = run(command, ROOT, log)
            if code:
                raise RuntimeError(f"Server exited with status {code}. See the error above and {LOG}.")
            return 0
        except KeyboardInterrupt:
            print("Local server startup interrupted.", file=sys.stderr)
            return 130
        except Exception as error:
            message = f"Local server failed: {type(error).__name__}: {error}"
            log.write(message + "\n")
            print(message, file=sys.stderr)
            print(f"Log: {LOG}", file=sys.stderr)
            return 1


if __name__ == "__main__":
    raise SystemExit(main())
