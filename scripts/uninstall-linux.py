#!/usr/bin/env python3
"""Remove ADHD startup and executables. Keep saved goals/settings unless manually removed."""
import os
from pathlib import Path
import shutil
import subprocess
import time

home = Path.home()
data = Path(os.environ.get('XDG_DATA_HOME', home / '.local/share'))
config = Path(os.environ.get('XDG_CONFIG_HOME', home / '.config'))
binary = home / '.local/bin/ADHD'
if binary.exists():
    subprocess.run([str(binary), '--quit'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
if shutil.which('systemctl'):
    subprocess.run(['systemctl', '--user', 'stop', 'adhd.service'], check=False)
if shutil.which('gnome-extensions'):
    subprocess.run(['gnome-extensions', 'disable', 'adhd-reminder@local'], check=False, capture_output=True)
if shutil.which('kwriteconfig6'):
    subprocess.run(['kwriteconfig6', '--file', 'kwinrc', '--group', 'Plugins', '--key', 'adhd-reminderEnabled', 'false'], check=False)
if shutil.which('qdbus6'):
    subprocess.run(['qdbus6', 'org.kde.KWin', '/KWin', 'reconfigure'], check=False)
time.sleep(2)
for file in [binary, config / 'autostart/io.github.adhd.Reminder.desktop', config / 'systemd/user/adhd.service', data / 'applications/io.github.adhd.Reminder.desktop']:
    file.unlink(missing_ok=True)
for folder in [data / 'adhd/install', config / 'adhd', data / 'gnome-shell/extensions/adhd-reminder@local', data / 'kwin/scripts/adhd-reminder']:
    if folder.is_dir(): shutil.rmtree(folder)
if shutil.which('systemctl'):
    subprocess.run(['systemctl', '--user', 'daemon-reload'], check=False)
print('ADHD uninstalled. Saved goals/settings are retained in your user data directory.')
