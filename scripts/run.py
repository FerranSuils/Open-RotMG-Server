#!/usr/bin/env python3
import argparse
import hashlib
from pathlib import Path
import shutil
import signal
import subprocess
import sys
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
REVISION = "b4fb2a94a06077dab6d01abce3cddd5f64e5416a"
URL = f"https://raw.githubusercontent.com/OpenRotMG/Open-RotMG-Client/{REVISION}/client.swf"
SHA256 = "07ae4fc3ca021a5bf08bbf94f057f7620be18b85e01abd763b3a198382e8fa48"
LOG = ROOT / ".local/server.log"


def prepare_client():
    path = ROOT / ".local/hosted-client/client.swf"
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.exists():
        data = path.read_bytes()
    else:
        print(f"Downloading pinned reference client: {URL}", flush=True)
        try:
            with urlopen(URL, timeout=60) as response:
                data = response.read()
        except Exception as error:
            raise RuntimeError(f"Download {URL} failed: {type(error).__name__}: {error}") from error
    actual = hashlib.sha256(data).hexdigest()
    if actual != SHA256:
        raise RuntimeError(
            f"Client checksum mismatch: expected {SHA256}, got {actual}. "
            f"Refusing to run. If the cached file is damaged, remove {path} and rerun."
        )
    if not path.exists():
        path.write_bytes(data)
    print(f"Verified reference client: {SHA256}", flush=True)


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


def main():
    parser = argparse.ArgumentParser(description="Start the local server with the client hosted at openrotmg.org.")
    parser.add_argument("--local-client", type=Path, metavar="DIR", help="Explicitly use a local client checkout instead of the hosted page")
    parser.add_argument("--dev-control", action="store_true", help="Enable loopback-only QA control")
    args = parser.parse_args()
    if shutil.which("cargo") is None:
        parser.error("Cargo not found. Install Rust/Cargo from https://rustup.rs/ and rerun.")
    ROOT.joinpath(".local").mkdir(mode=0o700, exist_ok=True)
    with LOG.open("a") as log:
        try:
            if args.local_client is not None:
                client = args.local_client.resolve()
                prepare = client / "scripts/run.py"
                if not prepare.is_file():
                    raise RuntimeError(f"Missing client launcher: {prepare}. Clone https://github.com/OpenRotMG/Open-RotMG-Client into {client}.")
                print(f"Preparing your local client at {client}. Logs: {LOG}", flush=True)
                if run([sys.executable, str(prepare), "--prepare-only"], client, log):
                    raise RuntimeError(f"Local client preparation failed. See the error above and {LOG}.")
                client_args = ["--client", str(client)]
            else:
                prepare_client()
                client_args = ["--hosted-client"]
            print(f"Building and starting your local server. Logs: {LOG}", flush=True)
            command = ["cargo", "run", "--locked", "--", *client_args]
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
