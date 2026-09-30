#!/bin/sh
set -eu
adhd_test_tmp=$(mktemp -d -t adhd-soak-display-XXXXXX)
Xvfb -displayfd 3 -screen 0 1280x800x24 -nolisten tcp 3>"$adhd_test_tmp/display" >"$adhd_test_tmp/xvfb.log" 2>&1 &
adhd_xvfb_pid=$!
trap 'kill "$adhd_xvfb_pid" 2>/dev/null || true; rm -rf -- "$adhd_test_tmp"' EXIT
adhd_tries=0
while [ ! -s "$adhd_test_tmp/display" ]; do
    adhd_tries=$((adhd_tries + 1))
    if [ "$adhd_tries" -gt 50 ]; then cat "$adhd_test_tmp/xvfb.log" >&2; exit 1; fi
    sleep 0.1
done
DISPLAY=":$(cat "$adhd_test_tmp/display")" XDG_SESSION_TYPE=x11 XDG_CURRENT_DESKTOP= WINIT_X11_SCALE_FACTOR=1 \
    dbus-run-session -- python3 scripts/soak-linux.py "$@"
