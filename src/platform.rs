use adhd::core::Desktop;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub fn watch(changed: impl Fn(Desktop) + Send + 'static) -> Arc<Mutex<Desktop>> {
    let current = Arc::new(Mutex::new(Desktop {
        locked: true,
        description: "Checking desktop integration…".into(),
        ..Default::default()
    }));
    let shared = current.clone();
    std::thread::spawn(move || {
        let mut probe = native::Probe::new();
        loop {
            let next = probe.sample();
            let different = if let Ok(mut guard) = shared.lock() {
                if *guard != next {
                    *guard = next.clone();
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if different {
                changed(next);
            }
            // Desktop state only; the UI and reminder scheduler do not redraw or poll here.
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    current
}

pub fn notify(summary: &str, body: &str) {
    native::notify(summary, body);
}

pub fn prepare_window(window: &slint::winit_030::winit::window::Window, manual: bool) {
    #[cfg(target_os = "linux")]
    {
        use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use x11rb::{
            connection::Connection,
            protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, PropMode},
            wrapper::ConnectionExt as _,
        };
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Xlib(h) = handle.as_raw() {
                if let Ok((x, screen)) = x11rb::connect(None) {
                    if let Ok(cookie) = x.intern_atom(false, b"_NET_WM_USER_TIME") {
                        if let Ok(atom) = cookie.reply() {
                            if !manual {
                                let _ = x.change_property32(
                                    PropMode::REPLACE,
                                    h.window as u32,
                                    atom.atom,
                                    AtomEnum::CARDINAL,
                                    &[0],
                                );
                            } else {
                                let _ = x.delete_property(h.window as u32, atom.atom);
                            }
                            let _ = x.flush();
                        }
                    }
                    // Identify a normal application explicitly instead of relying on the
                    // window manager's handling of winit's legacy source-0 hint.
                    let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
                        let state = x.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
                        let above = x.intern_atom(false, b"_NET_WM_STATE_ABOVE")?.reply()?.atom;
                        let event = ClientMessageEvent::new(
                            32,
                            h.window as u32,
                            state,
                            [1, above, 0, 1, 0],
                        );
                        x.send_event(
                            false,
                            x.setup().roots[screen].root,
                            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                            event,
                        )?;
                        x.get_input_focus()?.reply()?; // Barrier before a map request on winit's connection.
                        Ok(())
                    })();
                }
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = (window, manual);
    }
}

#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use std::{collections::HashMap, time::Instant};
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt},
        rust_connection::RustConnection,
    };
    use zbus::blocking::{connection::Builder, Connection as Bus, Proxy};

    #[derive(Default)]
    struct Report {
        when: Option<Instant>,
        fullscreen: bool,
        locked: bool,
        desktop: String,
    }
    struct Bridge(Arc<Mutex<Report>>);

    #[zbus::interface(name = "io.github.adhd.Desktop")]
    impl Bridge {
        fn report(&self, fullscreen: bool, locked: bool, desktop: &str) {
            if let Ok(mut r) = self.0.lock() {
                *r = Report {
                    when: Some(Instant::now()),
                    fullscreen,
                    locked,
                    desktop: desktop.chars().take(32).collect(),
                };
            }
        }
    }

    pub struct Probe {
        bus: Option<Bus>,
        report: Arc<Mutex<Report>>,
        x: Option<(RustConnection, u32)>,
        wayland: bool,
    }

    impl Probe {
        pub fn new() -> Self {
            let report = Arc::new(Mutex::new(Report::default()));
            let bus = Builder::session()
                .ok()
                .and_then(|b| b.name("io.github.adhd.Desktop").ok())
                .and_then(|b| {
                    b.serve_at("/io/github/adhd/Desktop", Bridge(report.clone()))
                        .ok()
                })
                .and_then(|b| b.build().ok())
                .or_else(|| Bus::session().ok());
            let x = x11rb::connect(None).ok().map(|(c, screen)| {
                let root = c.setup().roots[screen].root;
                (c, root)
            });
            Self {
                bus,
                report,
                x,
                wayland: std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland"),
            }
        }

        pub fn sample(&mut self) -> Desktop {
            let lock = self.locked();
            if let Ok(r) = self.report.lock() {
                if r.when
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(40))
                {
                    return Desktop {
                        locked: lock.unwrap_or(r.locked) || r.locked,
                        fullscreen: r.fullscreen,
                        reliable_fullscreen: true,
                        description: format!(
                            "{} helper connected · full-screen detection available",
                            r.desktop
                        ),
                    };
                }
            }
            Desktop {
                locked: lock.unwrap_or(false),
                fullscreen: self.x_fullscreen().unwrap_or(false),
                reliable_fullscreen: !self.wayland && self.x.is_some(),
                description: if self.wayland {
                    "Wayland: desktop helper unavailable. Full-screen detection is limited; reminders also use desktop notifications. Open ADHD at any time.".into()
                } else if self.x.is_some() {
                    "X11 · full-screen detection and floating windows available".into()
                } else {
                    "Desktop integration unavailable. Open ADHD to recover your reminder.".into()
                },
            }
        }

        fn locked(&self) -> Option<bool> {
            let bus = self.bus.as_ref()?;
            let names = Proxy::new(
                bus,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
            )
            .ok()?;
            let mut detected = None;
            for (service, path) in [
                ("org.gnome.ScreenSaver", "/org/gnome/ScreenSaver"),
                ("org.freedesktop.ScreenSaver", "/ScreenSaver"),
            ] {
                // Never activate another desktop's screensaver just to query it.
                if !names
                    .call::<_, _, bool>("NameHasOwner", &(service,))
                    .unwrap_or(false)
                {
                    continue;
                }
                if let Ok(p) = Proxy::new(bus, service, path, service) {
                    if let Ok(active) = p.call::<_, _, bool>("GetActive", &()) {
                        if active {
                            return Some(true);
                        }
                        detected = Some(false);
                    }
                }
            }
            detected
        }

        fn x_fullscreen(&self) -> Option<bool> {
            let (x, root) = self.x.as_ref()?;
            let atom = |name: &[u8]| {
                x.intern_atom(false, name)
                    .ok()?
                    .reply()
                    .ok()
                    .map(|r| r.atom)
            };
            let active = atom(b"_NET_ACTIVE_WINDOW")?;
            let state = atom(b"_NET_WM_STATE")?;
            let full = atom(b"_NET_WM_STATE_FULLSCREEN")?;
            let reply = x
                .get_property(false, *root, active, AtomEnum::WINDOW, 0, 1)
                .ok()?
                .reply()
                .ok()?;
            let window = reply.value32()?.next()?;
            let reply = x
                .get_property(false, window, state, AtomEnum::ATOM, 0, 64)
                .ok()?
                .reply()
                .ok()?;
            let fullscreen = reply.value32()?.any(|a| a == full);
            Some(fullscreen)
        }
    }

    pub fn notify(summary: &str, body: &str) {
        let (summary, body) = (summary.to_owned(), body.to_owned());
        std::thread::spawn(move || {
            let Ok(bus) = Bus::session() else { return };
            let Ok(proxy) = Proxy::new(
                &bus,
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
            ) else {
                return;
            };
            let mut hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
            hints.insert("suppress-sound", true.into());
            hints.insert("desktop-entry", "io.github.adhd.Reminder".into());
            let _: Result<u32, _> = proxy.call(
                "Notify",
                &(
                    "ADHD",
                    0u32,
                    "preferences-system-time",
                    summary,
                    body,
                    Vec::<String>::new(),
                    hints,
                    0i32,
                ),
            );
        });
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows_sys::Win32::{
        Foundation::RECT,
        System::StationsAndDesktops::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    };
    pub struct Probe;
    impl Probe {
        pub fn new() -> Self {
            Self
        }
        pub fn sample(&mut self) -> Desktop {
            // Input desktop access is denied on the lock/security desktop.
            let locked = unsafe {
                let desktop = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
                if desktop.is_null() {
                    true
                } else {
                    let available = SwitchDesktop(desktop) != 0;
                    CloseDesktop(desktop);
                    !available
                }
            };
            let fullscreen = unsafe {
                let window = GetForegroundWindow();
                let mut rect: RECT = std::mem::zeroed();
                let mut state = 0;
                let notification_fullscreen = SHQueryUserNotificationState(&mut state) >= 0
                    && matches!(
                        state,
                        QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE | QUNS_BUSY
                    );
                let is_app =
                    !window.is_null() && window != GetDesktopWindow() && window != GetShellWindow();
                let monitor = windows_sys::Win32::Graphics::Gdi::MonitorFromWindow(
                    window,
                    windows_sys::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
                );
                let mut info: windows_sys::Win32::Graphics::Gdi::MONITORINFO = std::mem::zeroed();
                info.cbSize = std::mem::size_of_val(&info) as u32;
                let covers = is_app
                    && GetWindowRect(window, &mut rect) != 0
                    && windows_sys::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info) != 0
                    && rect.left <= info.rcMonitor.left
                    && rect.top <= info.rcMonitor.top
                    && rect.right >= info.rcMonitor.right
                    && rect.bottom >= info.rcMonitor.bottom;
                notification_fullscreen || covers
            };
            Desktop { locked, fullscreen, reliable_fullscreen: true, description: "Windows · full-screen detection available. Exclusive full-screen apps may cover Serious mode.".into() }
        }
    }
    pub fn notify(summary: &str, body: &str) {
        let summary: Vec<u16> = summary.encode_utf16().chain(Some(0)).collect();
        let body: Vec<u16> = body.encode_utf16().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                summary.as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}
