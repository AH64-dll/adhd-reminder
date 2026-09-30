# ADHD — one thing at a time

A small Rust desktop app that keeps your current goal nearby without constant notifications.

[Download the Linux or Windows release](https://github.com/AH64-dll/adhd-reminder/releases/latest). Each bundle includes its installer and instructions.

Run `ADHD` or open **ADHD** in your application launcher. Enter one goal. The card stays for five seconds, then hides. By default a check-in appears after a fresh random interval of 20–30 minutes and stays until you answer.

- **Still on it** hides the card and starts a fresh interval.
- **Finished** asks what you will do now.
- **Canceled** asks what you will do instead.
- **Hide** or the window close button hides the card and schedules the next check-in. A hidden next-goal prompt waits until you reopen ADHD.
- **Quit** stops ADHD for this session. It returns at the next login unless you turn off **Start when I log in**.
- Settings supports a fixed interval or custom random range (1–1440 whole minutes).
- **Normal mode** waits for full-screen activity to end. **Serious mode** tries to show above it. Both defer reminders while locked.

Goals and drafts remain on your device. No account, analytics, network service, or task history is used. The app listens only on an authenticated loopback socket for its own launcher.

## Install

### Linux

From a release bundle:

```sh
python3 scripts/install-linux.py --binary bin/adhd
```

From source:

```sh
cargo build --release --locked
python3 scripts/install-linux.py
```

The installer places `ADHD` in `~/.local/bin`, adds an application entry and login startup, and starts it. Ensure `~/.local/bin` is on your terminal's `PATH`. No root access is required for installing the app. `--no-start` skips launching; `--no-helper` skips the desktop helper.

Build prerequisites: Rust (use current stable), C compiler, pkg-config, fontconfig, xkbcommon, and X11/Wayland development libraries. On Fedora:

```sh
sudo dnf install gcc gcc-c++ fontconfig-devel libxkbcommon-devel libX11-devel libXcursor-devel libXrandr-devel libXi-devel libxcb-devel libXfixes-devel wayland-devel
```

On Ubuntu/Debian:

```sh
sudo apt install build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxcb1-dev libxfixes-dev libwayland-dev
```

The bundled x86-64 Linux binary is built in Debian Bookworm and requires glibc 2.35 or later, fontconfig, and the desktop's X11/Wayland libraries. Its GLIBC symbol requirements were inspected, and the bundle was tested on Fedora 44. Build from source for older systems or other CPU architectures.

### Windows 10 and 11

1. Download the **Windows ZIP** from the [latest release](https://github.com/AH64-dll/adhd-reminder/releases/latest).
2. Right-click the ZIP and choose **Extract All**.
3. Open the extracted folder and double-click **Setup.cmd**.
4. Approve the Windows administrator prompt. Setup installs ADHD and starts it.

No terminal commands, Rust, or extra PowerShell installation are needed. Keep `Setup.cmd`, `scripts`, and `bin` together until setup finishes. Double-click Setup normally; it requests permission itself.

Administrator approval is used only to register login startup. Files, shortcuts, and PATH belong to the person who clicked Setup, and the reminder runs with ordinary user permissions. This also applies when a different administrator approves the prompt. Canceling the permission prompt leaves setup incomplete and shows an explanation in the original window.

For manual installation or source builds, the PowerShell entry point is still available:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1 -Binary .\bin\adhd.exe
```

The per-user installer adds a Start menu entry, a login task, and the app folder to your user PATH. Open a new terminal to use `ADHD`. Keep any bundled DLL files beside the executable. From source, use a Rust MSVC toolchain with Visual Studio C++ Build Tools and `cargo build --release --locked`.

### Other distribution options

| Platform | Option | User experience and tradeoff |
| --- | --- | --- |
| Windows | `Setup.cmd` (included) | Extract a ZIP, double-click Setup, approve permission. Small and easy to maintain. |
| Windows | [Inno Setup `Setup.exe`](https://jrsoftware.org/isinfo.php) | A single download with a familiar installation wizard and uninstaller. Recommended next step for general distribution. |
| Windows | MSI | Useful for centrally managed company devices; more packaging work for this small app. |
| Windows | [WinGet](https://learn.microsoft.com/en-us/windows/package-manager/winget/) | Install and update through Windows Package Manager after publishing an appropriate package. ADHD is not listed yet. |
| Linux | Current per-user script (included) | One command, no root access, desktop launcher and login startup installed together. |
| Linux | [AppImage](https://docs.appimage.org/introduction/concepts.html) | One portable file. Users may need to mark it executable; login startup and helpers still need integration. |
| Linux | [Flatpak](https://docs.flatpak.org/en/latest/using-flatpak.html) | Install through supported software centers; requires packaging and adapting desktop/startup access to the sandbox. |
| Linux | `.deb` / `.rpm` | Native distribution packages and system package management; maintain separate Debian/Ubuntu and Fedora builds. |

Only the ZIP/command-file and existing Linux script are included in this release; the other formats are future options.

## Linux desktop integration

The Rust app runs independently of the optional helpers. Linux X11 uses standard window-manager hints. When XWayland is available it is preferred for floating-window placement; native Wayland remains a fallback.

- **GNOME Wayland:** `integrations/gnome` reports full-screen/lock state and keeps the app above ordinary windows and on all workspaces. The installer copies it to your extensions directory and enables it. A newly installed extension may need a logout/login before GNOME discovers it. Settings shows whether the helper is actually connected.
- **KDE Plasma 6 Wayland:** `integrations/kwin` provides a KWin script. Enable it in **System Settings → KWin Scripts** if it is not active after installation.
- **Other Wayland desktops:** ordinary windows and quiet notification fallback are available. Full-screen detection and exact positioning may be unavailable. Settings reports these limitations; you can always reopen with `ADHD`.

Helpers report only desktop state. No goal text crosses their D-Bus interface. Missing/stale helpers lose their connected status after 40 seconds. Serious mode cannot cover protected system screens or every exclusive full-screen game.

## Recovery and storage

The UI keeps running while hidden. The launcher enforces one instance per data directory. A small supervisor restarts crashes with backoff and checks responsiveness every 30 seconds; three missed responses restart a hung UI. Five rapid failures stop retrying and show a diagnostic notice. A clean Quit is never automatically restarted by the supervisor.

State changes use atomic file replacement with a valid backup. A damaged primary file is preserved as `state.corrupt.json`. A recovered goal is checked once after restart, not once for every missed interval. A disk-write failure remains visible; it does not discard the in-memory goal.

Default data locations:

- Linux: `${XDG_DATA_HOME:-~/.local/share}/adhd`
- Windows: `%LOCALAPPDATA%\adhd\adhd\data`

Files: `state.json`, `state.backup.json`, a per-user IPC endpoint/lock, and bounded diagnostic logs (two files, approximately 256 KiB each). Diagnostic logs do not contain goal text.

Useful commands:

```sh
ADHD --status
ADHD --show
ADHD --hide
ADHD --check-now
ADHD --quit
```

Use **Reset window position** if a display layout changes. A fresh launch also checks saved coordinates against currently available monitors.

## Verify

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The Linux GUI smoke test uses actual clicks/typing and a disposable data directory. With Xvfb, xdotool, and ImageMagick installed:

```sh
Xvfb :93 -screen 0 1280x800x24 -nolisten tcp &
DISPLAY=:93 XDG_SESSION_TYPE=x11 XDG_CURRENT_DESKTOP= WINIT_X11_SCALE_FACTOR=1 dbus-run-session -- python3 scripts/smoke-linux.py --binary target/release/adhd
```

`--test-interval-seconds` is accepted only with an explicit isolated `--data-dir`. Regular settings never permit sub-minute intervals. The accelerated 24-hour unit simulation tests scheduling logic; it is not a substitute for a real 24-hour resource soak. See `VERIFICATION.md` for measured results and untested platforms.

For the full elapsed-time test, run `sh scripts/run-soak-linux.sh --binary target/release/adhd --seconds 86400`. It creates an isolated display and disposable goal data, and writes progress to `test-results/soak.json`. Keep the device awake for this test. When launched as the `adhd-soak-check` user service, stop only the test with `systemctl --user stop adhd-soak-check`; your normal reminder remains running.

The release build can be reproduced with `scripts/Containerfile.linux` using Podman or Docker. Mount the project at `/work` and a writable build directory at `/build`. Package built binaries with `python3 scripts/package.py --linux-binary target/portable/release/adhd`; the Windows binary is read from `target/x86_64-pc-windows-gnu/release/adhd.exe`. Bundles include dependency license notices and SHA-256 checksums.

## Uninstall

Linux: `python3 scripts/uninstall-linux.py`

Windows: double-click **Uninstall.cmd** in the extracted bundle, or run `powershell -ExecutionPolicy Bypass -File .\scripts\uninstall-windows.ps1`.

Both remove startup integration and the app while retaining saved goals/settings. Delete the data directory separately if you want to erase them.

## Source layout and licensing

`core` owns goal/timing transitions; `storage` owns atomic persistence; `ipc` owns single-instance commands. The executable connects Slint, desktop adapters, startup controls, and supervision. No web runtime is embedded. Rendering is software-based and occurs on UI changes; desktop-state probes run every two seconds, independently of the timer.

Application source is MIT licensed. Slint is used under its Royalty-free Desktop, Mobile, and Web Applications License; attribution is available in **Settings → About**. Dependencies retain their own licenses. See `THIRD_PARTY.md`.
