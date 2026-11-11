#!/usr/bin/env python3
"""Profile an empty home page with an isolated, local mock API (Linux desktop required)."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('memory', HERE / 'profile-memory.py')
memory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(memory)


class EmptyApi(BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length', 0)))
        body = {'code': 200, 'result': [], 'recommend': [], 'data': {'dailySongs': []},
                'sub': [], 'banners': [], 'albums': [], 'list': []}
        data = json.dumps({'status': 200, 'body': body, 'cookie': []}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path, help='Slint binary, or Electron binary with --electron')
    parser.add_argument('--electron', action='store_true')
    parser.add_argument('--renderer', choices=['vulkan', 'opengl', 'software', 'all'], default='all')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    server = ThreadingHTTPServer(('127.0.0.1', 0), EmptyApi)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    cases = ['electron'] if args.electron else (
        ['vulkan', 'opengl', 'software'] if args.renderer == 'all' else [args.renderer])
    results = []
    try:
        for renderer in cases:
            with tempfile.TemporaryDirectory(prefix='ncm-memory-fixture-') as temporary:
                config = Path(temporary) / 'config'
                config.mkdir()
                (config / 'preferences.json').write_text(json.dumps({'renderer': renderer}))
                env = os.environ.copy()
                env.update(NCM_DATA_DIR=str(config), NCM_CACHE_DIR=temporary + '/cache',
                           NCM_API_URL=f'http://127.0.0.1:{server.server_port}',
                           NCM_MEMORY_USER_DATA=temporary + '/electron')
                command = [str(binary)]
                if args.electron:
                    command.append(str(HERE / 'profile-memory-electron.cjs'))
                logfile = args.output.with_suffix(f'.{renderer}.log')
                with logfile.open('w') as log:
                    process = subprocess.Popen(command, env=env, stdout=log, stderr=log,
                                               start_new_session=True)
                    try:
                        for delay in ([15, 5] if args.electron else [8, 5]):
                            time.sleep(delay)
                            if process.poll() is not None:
                                raise RuntimeError(f'{renderer} exited; inspect {logfile}')
                            sample = memory.snapshot(process.pid)
                            sample.update(renderer=renderer, binary=str(binary))
                            try:
                                windows = json.loads(subprocess.check_output(
                                    ['niri', 'msg', '-j', 'windows'], text=True, timeout=3))
                                sample['window_layouts'] = [w['layout'] for w in windows
                                                           if w.get('pid') == process.pid]
                            except (OSError, ValueError, subprocess.SubprocessError):
                                pass
                            results.append(sample)
                            args.output.write_text(json.dumps(results, indent=2) + '\n')
                            print(renderer, sample['total'], flush=True)
                    finally:
                        if process.poll() is None:
                            # Let Electron close its own GPU/zygote children gracefully.
                            process.terminate()
                            try:
                                process.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                os.killpg(process.pid, signal.SIGKILL)
                                process.wait()
    finally:
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    main()
