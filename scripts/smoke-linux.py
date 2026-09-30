#!/usr/bin/env python3
"""Real mouse/keyboard checks in an isolated X server. Requires Xvfb, xdotool, ImageMagick.
Run under dbus-run-session with DISPLAY pointing at the disposable X server.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, default=Path('target/debug/adhd'))
parser.add_argument('--output', type=Path, default=Path('test-results'))
args = parser.parse_args()
binary = args.binary.resolve()
args.output.mkdir(parents=True, exist_ok=True)
results = []

def wait_for(predicate, timeout=15):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            last = predicate()
            if last: return last
        except (OSError, ValueError, subprocess.SubprocessError):
            pass
        time.sleep(0.15)
    raise AssertionError(f'Condition did not become true in {timeout}s; last={last!r}')

def x(*args):
    return subprocess.check_output(['xdotool', *map(str, args)], text=True).strip()

with tempfile.TemporaryDirectory(prefix='adhd-smoke-') as directory:
    data = Path(directory)
    common = [str(binary), '--data-dir', str(data)]
    log = open(args.output / 'smoke-runtime.log', 'w')
    process = subprocess.Popen(common + ['--supervise', '--test-interval-seconds', '12'], stdout=log, stderr=log)
    def command(name):
        return subprocess.check_output(common + ['--' + name], text=True, stderr=subprocess.DEVNULL).strip()
    def status(): return json.loads(command('status'))
    def state(): return json.loads((data / 'state.json').read_text())
    def window(): return wait_for(lambda: x('search', '--class', '^ADHD$').splitlines()[-1], 5)
    def click(px, py):
        w = window()
        x('windowfocus', '--sync', w)
        x('mousemove', '--window', w, px, py, 'click', 1)
        time.sleep(0.2)
    def capture(name):
        subprocess.run(['import', '-window', window(), str(args.output / name)], check=True)
    def enter(goal):
        click(120, 193 if state()['phase'] == {'Entry': 'Canceled'} else 162)
        x('key', 'ctrl+a')
        x('type', '--clearmodifiers', '--delay', 2, goal)
        x('key', 'Return')
        wait_for(lambda: status()['has_goal'])
    try:
        wait_for(lambda: status()['visible'])
        capture('entry.png')
        enter('Write the first paragraph')
        capture('reminder.png')
        wait_for(lambda: not status()['visible'], 8)
        wait_for(lambda: status()['visible'] and status()['phase'] == 'Pending', 12)
        time.sleep(2)
        assert status()['visible'], 'Unanswered reminder must stay visible'
        results.append('Entry, 5-second preview, hide, timed return, unanswered persistence')

        before = status()['pid']
        subprocess.run(common, check=True)
        assert status()['pid'] == before
        results.append('Repeated launch restores the existing instance')

        click(80, 230)
        wait_for(lambda: not status()['visible'])
        assert state()['goal'] == 'Write the first paragraph'
        results.append('Still on it hides and resets without losing the goal')

        command('show')
        click(208, 230)
        wait_for(lambda: state()['phase'] == {'Entry': 'Finished'})
        capture('finished.png')
        enter('Check the references')
        click(332, 230)
        wait_for(lambda: state()['phase'] == {'Entry': 'Canceled'})
        capture('canceled.png')
        results.append('Finished and Canceled ask their distinct next-goal questions')

        click(355, 40)
        capture('settings.png')
        # Save default values; tests below inject compositor reports, not app state.
        click(355, 40)
        enter('Test recovery')
        command('hide')
        pid = status()['pid']
        os.kill(pid, signal.SIGKILL)
        wait_for(lambda: status()['pid'] != pid and status()['visible'], 20)
        assert state()['goal'] == 'Test recovery'
        assert status()['phase'] == 'Pending'
        results.append('Supervisor recovers a killed app with the saved goal')

        # The optional helpers use exactly this D-Bus interface.
        def desktop(full, locked=False):
            subprocess.run(['gdbus', 'call', '--session', '--dest', 'io.github.adhd.Desktop',
                '--object-path', '/io/github/adhd/Desktop', '--method', 'io.github.adhd.Desktop.Report',
                str(full).lower(), str(locked).lower(), 'Test desktop'], check=True, stdout=subprocess.DEVNULL)
        desktop(True)
        wait_for(lambda: not status()['visible'])
        assert status()['phase'] == 'Pending'
        desktop(False, True)
        time.sleep(2.5)
        assert not status()['visible']
        desktop(False, False)
        wait_for(lambda: status()['visible'])
        results.append('Normal mode defers full-screen and lock, then restores one pending reminder')

        click(355, 40)
        click(32, 122)  # Switch to a fixed interval; the second range row disappears.
        click(220, 157)
        x('key', 'ctrl+a')
        x('type', '7')
        x('key', 'Tab')
        click(32, 192)
        capture('serious-settings.png')
        click(200, 372)
        wait_for(lambda: state()['settings']['serious'] and state()['settings']['fixed_minutes'] == 7)
        assert not state()['settings']['random']
        command('hide')
        desktop(True)
        command('check-now')
        wait_for(lambda: status()['visible'])
        desktop(True, True)
        wait_for(lambda: not status()['visible'])
        desktop(True, False)
        wait_for(lambda: status()['visible'])
        results.append('Fixed custom interval persists; Serious mode shows during full-screen but defers lock')

        command('quit')
        process.wait(timeout=10)
        assert process.returncode == 0
        time.sleep(2)
        assert subprocess.run(common + ['--status'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode != 0
        results.append('Explicit Quit exits supervisor and does not restart')
    finally:
        if process.poll() is None:
            try: command('quit'); process.wait(timeout=8)
            except (OSError, subprocess.SubprocessError): process.terminate(); process.wait(timeout=5)
        log.close()

(args.output / 'smoke.json').write_text(json.dumps({'passed': results}, indent=2))
print(json.dumps({'passed': results}, indent=2))
