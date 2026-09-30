#!/usr/bin/env python3
"""Freeze an isolated UI process and verify supervisor recovery. Takes about two minutes."""
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import tempfile
import time

binary = Path('target/release/adhd').resolve()
output = Path('test-results/watchdog.json')
output.parent.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='adhd-watchdog-') as directory:
    installed = Path(directory, 'ADHD')
    shutil.copy2(binary, installed)
    command = [str(installed), '--data-dir', directory]
    parent = subprocess.Popen(command + ['--supervise'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    def status(): return json.loads(subprocess.check_output(command + ['--status'], text=True, stderr=subprocess.DEVNULL))
    old_pid = None
    try:
        for _ in range(30):
            try: old_pid = status()['pid']; break
            except (ValueError, subprocess.SubprocessError): time.sleep(0.2)
        assert old_pid, 'App did not start'
        os.kill(old_pid, signal.SIGSTOP)
        replacement = Path(directory, 'ADHD.new')
        shutil.copy2(binary, replacement)
        replacement.replace(installed)  # Exercise an atomic app update during a hang.
        started = time.monotonic()
        result = {'status': 'running', 'frozen_pid': old_pid}
        output.write_text(json.dumps(result, indent=2))
        while time.monotonic() - started < 135:
            try: os.kill(old_pid, 0)
            except ProcessLookupError: break
            time.sleep(2)
        else: raise AssertionError('Supervisor did not terminate the frozen UI')
        for _ in range(60):
            try:
                new = status()
                if new['pid'] != old_pid: break
            except (ValueError, subprocess.SubprocessError): pass
            time.sleep(0.3)
        else: raise AssertionError('Supervisor did not restart a responsive UI')
        result.update(status='passed', recovered_pid=new['pid'], recovery_seconds=round(time.monotonic()-started, 1), atomic_update=True)
        output.write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2))
    except BaseException as error:
        output.write_text(json.dumps({'status':'failed', 'error':str(error)}, indent=2))
        diagnostic = Path(directory, 'diagnostic.log')
        if diagnostic.exists(): output.with_suffix('.log').write_text(diagnostic.read_text())
        raise
    finally:
        if old_pid:
            try: os.kill(old_pid, signal.SIGCONT)
            except ProcessLookupError: pass
        subprocess.run(command + ['--quit'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try: parent.wait(timeout=8)
        except subprocess.TimeoutExpired: parent.terminate(); parent.wait(timeout=5)
