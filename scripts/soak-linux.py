#!/usr/bin/env python3
"""Real elapsed-time resource/visibility soak in an isolated X11 session.

Run with the release binary, on a disposable X display and D-Bus session.
Writes progress regularly; success is recorded only after the requested duration.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import signal

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, default=Path('target/release/adhd'))
parser.add_argument('--seconds', type=int, default=86400)
parser.add_argument('--output', type=Path, default=Path('test-results/soak.json'))
args = parser.parse_args()
def interrupted(_signum, _frame): raise SystemExit('Soak interrupted before completion')
signal.signal(signal.SIGTERM, interrupted)
args.output.parent.mkdir(parents=True, exist_ok=True)
clock_ticks = os.sysconf('SC_CLK_TCK')
page_size = os.sysconf('SC_PAGE_SIZE')

with tempfile.TemporaryDirectory(prefix='adhd-soak-') as directory:
    data = Path(directory)
    # A fixture goal only. The regular app never reads this directory.
    (data / 'state.json').write_text(json.dumps({
        'version': 1, 'goal': 'Isolated reliability test', 'draft': '', 'position': None,
        'settings': {'random': False, 'min_minutes': 20, 'max_minutes': 30, 'fixed_minutes': 1, 'serious': False},
        'phase': {'Waiting': {'due': int(time.time()) + 60}}
    }))
    common = [str(args.binary.resolve()), '--data-dir', str(data)]
    process = subprocess.Popen(common + ['--supervise'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    started = time.monotonic()
    record = {'status': 'running', 'requested_seconds': args.seconds, 'elapsed_seconds': 0,
              'cycles': 0, 'pids': [], 'samples': [], 'max_rss_mib': 0}
    def command(name): return subprocess.check_output(common + ['--' + name], text=True, stderr=subprocess.DEVNULL)
    def sample(pid):
        fields = Path(f'/proc/{pid}/stat').read_text().split()
        return (int(fields[13]) + int(fields[14])) / clock_ticks, int(fields[23]) * page_size / 1048576
    def write(): args.output.write_text(json.dumps(record, indent=2))
    previous = None
    next_hide = 0
    try:
        time.sleep(3)
        command('hide')
        while time.monotonic() - started < args.seconds:
            state = json.loads(command('status'))
            pid = state['pid']
            if pid not in record['pids']: record['pids'].append(pid)
            cpu, rss = sample(pid)
            supervisor_cpu, supervisor_rss = sample(process.pid)
            cpu += supervisor_cpu
            rss += supervisor_rss
            now = time.monotonic()
            percent = None if previous is None else (cpu - previous[1]) / (now - previous[0]) * 100
            previous = (now, cpu)
            record['max_rss_mib'] = max(record['max_rss_mib'], rss)
            record['elapsed_seconds'] = round(now - started, 1)
            if not record['samples'] or record['elapsed_seconds'] - record['samples'][-1]['second'] >= 60 or args.seconds <= 180:
                record['samples'].append({'second': record['elapsed_seconds'], 'cpu_percent': percent, 'rss_mib': round(rss, 2)})
            if state['phase'] == 'Pending':
                if not state['visible']: raise AssertionError('Due reminder remained hidden on an unlocked X11 test desktop')
                command('hide')
                record['cycles'] += 1
                next_hide = now
            elif now - next_hide > 90 and next_hide:
                raise AssertionError('Reminder failed to become due')
            write()
            time.sleep(min(10, max(0.1, args.seconds - (time.monotonic() - started))))
        record['status'] = 'passed'
        record['elapsed_seconds'] = round(time.monotonic() - started, 1)
    except BaseException as error:
        record['status'] = 'failed'
        record['error'] = str(error)
        raise
    finally:
        write()
        try: command('quit'); process.wait(timeout=8)
        except (OSError, subprocess.SubprocessError): process.terminate(); process.wait(timeout=5)
print(json.dumps(record, indent=2))
