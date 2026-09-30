# Verification

Implementation and local checks: September 29–30, 2026. This records tested behavior, not a guarantee of zero future bugs.

## Completed

- Rust formatting and Clippy with warnings denied.
- Nine unit tests: state transitions, distinct completion prompts, random/fixed timing, lock/full-screen policy, restart and clock changes, an accelerated 24-hour scheduler simulation, atomic storage/backup recovery, storage failure, and single-instance authenticated IPC.
- Real mouse/keyboard interaction in an isolated X11 session: goal entry, five-second preview, hide, timed return, unanswered reminders, all three goal buttons, settings, custom fixed timing, duplicate launch, crash recovery, and intentional Quit.
- Normal/Serious/lock policy integration through the same D-Bus interface used by desktop helpers. These policy tests inject desktop reports; they do not substitute for running each real compositor.
- Actual GNOME 50 Wayland/XWayland window capture and timed-reminder keyboard-focus preservation. After login, the GNOME helper reported live desktop state and the window had above/sticky window-manager properties.
- Linux per-user installation, application launcher, systemd service validation, and automatic startup observed after a real login.
- Portable Linux release built in Debian Bookworm, GLIBC symbols inspected (maximum required version 2.35), and the full isolated GUI interaction suite passed using that binary on Fedora 44.
- A frozen UI recovered through the supervisor in **97.1 seconds**, even when its executable was atomically replaced while frozen.
- Windows x86-64 cross-compilation and executable generation. DLL import inspection found only Windows system DLLs.
- Two-minute release-build soak: one real minute-long reminder cycle, about **30.4 MiB peak combined app/supervisor RSS** and **0.045% mean sampled CPU**. This is a short measurement, not a 24-hour result.
- Full elapsed-time release-build soak: **86,400 seconds passed**, **1,436 reminder cycles**, one UI process throughout, **30.21 MiB peak combined app/supervisor RSS**, and **0.0304% mean sampled CPU**. This used an isolated X11 display, disposable goal data, and one-minute reminder intervals. It does not substitute for Windows or physical suspend/monitor tests.

Screenshots and machine-readable reports are generated in `test-results/`. The 24-hour scheduler unit simulation is accelerated; elapsed-time soak results explicitly say `running`, `passed`, or `failed`.

## Required external/runtime checks

| Environment or scenario | Status |
| --- | --- |
| Current Fedora GNOME desktop, XWayland UI | Launched and visually inspected; focus preservation verified |
| GNOME helper | Active after login; live reports and above/sticky window properties verified |
| Isolated X11 interaction and policy checks | Passed |
| KDE Plasma 6 helper and real KDE desktop | Packaged; not runtime-tested here |
| Other Wayland compositors | Portable fallback implemented; not runtime-tested here |
| Windows 10 and Windows 11 UI, installer, login, sleep | Built; no Windows runtime available in this session |
| Physical monitor unplug, fractional scaling changes, real suspend/reboot | Recovery logic present; full hardware test matrix remains |
| Real 24-hour soak | Passed September 30: 86,400 seconds, 1,436 cycles, no UI restart; see measured results above |

Settings now shows **GNOME helper connected** on this device. Real full-screen applications, physical lock/unlock, and the other desktops still need their own runtime coverage; injected desktop-policy tests only verify the application's response to those reports.

## Reproduce

Run the commands in README for Rust and GUI checks. `scripts/check-watchdog-linux.py` freezes a disposable UI and atomically replaces its executable while checking for supervisor recovery. `scripts/run-soak-linux.sh --seconds 86400` supplies its own isolated X server for the full resource/visibility soak. Neither uses your actual goal data.

Linux release compatibility is determined from the actual bundled binary's GLIBC symbol requirements and library dependencies. The local Fedora build is distinct from the older-library container build used for distribution.
