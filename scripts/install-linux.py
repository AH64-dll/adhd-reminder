#!/usr/bin/env python3
"""Per-user installation. No root access or changes to existing shell configuration."""
import argparse
import ast
import os
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, default=root / 'target/release/adhd')
parser.add_argument('--no-start', action='store_true')
parser.add_argument('--no-helper', action='store_true')
args = parser.parse_args()
if not args.binary.is_file():
    sys.exit('Build first: cargo build --release --locked')

home = Path.home()
data = Path(os.environ.get('XDG_DATA_HOME', home / '.local/share'))
config = Path(os.environ.get('XDG_CONFIG_HOME', home / '.config'))
binary = home / '.local/bin/ADHD'
install = data / 'adhd/install'
for folder in [binary.parent, install, config / 'adhd', config / 'autostart', config / 'systemd/user', data / 'applications']:
    folder.mkdir(parents=True, exist_ok=True)

# Rename replaces an executable safely even if the old version is running.
temporary = binary.with_name('ADHD.new')
shutil.copy2(args.binary, temporary)
temporary.chmod(0o755)
temporary.replace(binary)

def desktop_quote(value):
    return '"' + str(value).replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%') + '"'

launcher = install / 'start-at-login.sh'
import shlex
launcher.write_text('#!/bin/sh\n'
    'if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then\n'
    '  systemctl --user import-environment DISPLAY WAYLAND_DISPLAY XAUTHORITY DBUS_SESSION_BUS_ADDRESS XDG_CURRENT_DESKTOP XDG_SESSION_TYPE\n'
    '  exec systemctl --user start adhd.service\n'
    'fi\nexec ' + shlex.quote(str(binary)) + ' --supervise\n')
launcher.chmod(0o755)

entry = '[Desktop Entry]\nType=Application\nName=ADHD\nComment=A quiet reminder of your current goal\nExec=' + desktop_quote(binary) + '\nIcon=preferences-system-time\nTerminal=false\nCategories=Utility;\nStartupWMClass=ADHD\n'
(data / 'applications/io.github.adhd.Reminder.desktop').write_text(entry)
autostart = entry.replace(desktop_quote(binary), desktop_quote(launcher)) + 'X-GNOME-Autostart-enabled=true\n'
(config / 'adhd/autostart.desktop').write_text(autostart)
(config / 'autostart/io.github.adhd.Reminder.desktop').write_text(autostart)
# systemd has its own quoting; escape percent specifiers in user paths.
exec_path = '"' + str(binary).replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%') + '"'
(config / 'systemd/user/adhd.service').write_text('[Unit]\nDescription=ADHD goal reminder\nAfter=graphical-session.target\nPartOf=graphical-session.target\nStartLimitIntervalSec=300\nStartLimitBurst=5\n\n[Service]\nType=simple\nExecStart=' + exec_path + ' --supervise\nRestart=on-failure\nRestartSec=10\nTimeoutStopSec=5\nKillMode=control-group\n')
(config / 'adhd/installed').write_text(str(binary))

if not args.no_helper:
    desktop = os.environ.get('XDG_CURRENT_DESKTOP', '').lower()
    if 'gnome' in desktop:
        destination = data / 'gnome-shell/extensions/adhd-reminder@local'
        shutil.copytree(root / 'integrations/gnome', destination, dirs_exist_ok=True)
        result = subprocess.run(['gnome-extensions', 'enable', 'adhd-reminder@local'], capture_output=True, text=True)
        if result.returncode:
            # New local extensions are discovered at the next Shell session. Preserve every
            # existing enabled extension and arrange activation when this one is discovered.
            current = subprocess.check_output(['gsettings', 'get', 'org.gnome.shell', 'enabled-extensions'], text=True).strip()
            enabled = ast.literal_eval(current.removeprefix('@as '))
            if 'adhd-reminder@local' not in enabled:
                enabled.append('adhd-reminder@local')
                subprocess.run(['gsettings', 'set', 'org.gnome.shell', 'enabled-extensions', repr(enabled)], check=True)
            print('GNOME helper installed and enabled for your next login. Full-screen detection is limited until then.')
    elif 'kde' in desktop:
        destination = data / 'kwin/scripts/adhd-reminder'
        shutil.copytree(root / 'integrations/kwin', destination, dirs_exist_ok=True)
        if shutil.which('kwriteconfig6'):
            subprocess.run(['kwriteconfig6', '--file', 'kwinrc', '--group', 'Plugins', '--key', 'adhd-reminderEnabled', 'true'], check=True)
        if shutil.which('qdbus6'):
            subprocess.run(['qdbus6', 'org.kde.KWin', '/KWin', 'reconfigure'], check=False)
        print('KDE helper installed. Enable it in System Settings → KWin Scripts if not active yet.')

if shutil.which('systemctl'):
    subprocess.run(['systemctl', '--user', 'daemon-reload'], check=False)
if not args.no_start:
    subprocess.Popen([str(launcher)], start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(f'Installed {binary}. Type ADHD or open it in your application launcher.')
if str(binary.parent) not in os.environ.get('PATH', '').split(os.pathsep):
    print(f'Add {binary.parent} to PATH to use ADHD in newly opened terminals.')
