#![cfg_attr(windows, windows_subsystem = "windows")]

mod platform;
mod startup;

use adhd::{
    core::{may_remind, Desktop, Phase, Settings, State},
    ipc,
    storage::Store,
};
use slint::{
    winit_030::{winit, WinitWindowAccessor},
    ComponentHandle, Timer, TimerMode,
};
use std::{
    cell::{Cell, RefCell},
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

slint::include_modules!();
thread_local! { static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) }; }

fn with_app(f: impl FnOnce(&Rc<App>)) {
    APP.with(|slot| {
        if let Some(app) = slot.borrow().as_ref() {
            f(app);
        }
    });
}

#[derive(Default)]
struct Args {
    data: Option<PathBuf>,
    run: bool,
    supervise: bool,
    command: Option<String>,
    test_interval: Option<i64>,
}

fn arguments() -> Result<Args, String> {
    let mut a = Args::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                a.data = Some(PathBuf::from(args.next().ok_or("Missing data directory")?))
            }
            "--run" => a.run = true,
            "--supervise" => a.supervise = true,
            "--show" | "--hide" | "--quit" | "--status" | "--check-now" => {
                a.command = Some(arg[2..].into())
            }
            "--test-interval-seconds" => {
                a.test_interval = Some(
                    args.next()
                        .ok_or("Missing test interval")?
                        .parse()
                        .map_err(|_| "Invalid test interval")?,
                )
            }
            "--help" | "-h" => {
                println!("ADHD — a quiet reminder of your current goal\n\nADHD               Open or restore the app\nADHD --status      Print running status\nADHD --hide        Hide without quitting\nADHD --check-now   Bring the current check-in forward\nADHD --quit        Quit until the next login\n\nTesting: --data-dir PATH --test-interval-seconds N --run");
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown option: {arg}")),
        }
    }
    if a.test_interval.is_some()
        && (a.data.is_none() || !a.test_interval.is_some_and(|n| (1..=3600).contains(&n)))
    {
        return Err("Test intervals require an isolated --data-dir and 1–3600 seconds".into());
    }
    Ok(a)
}

fn main() {
    if let Err(error) = launch() {
        eprintln!("ADHD: {error}");
        let diagnostic_command = std::env::args()
            .any(|a| matches!(a.as_str(), "--status" | "--quit" | "--hide" | "--check-now"));
        if !diagnostic_command {
            platform::notify(
                "ADHD could not start",
                &format!("{error}\nOpen ADHD again to retry."),
            );
            // Notification dispatch is asynchronous on Linux.
            std::thread::sleep(Duration::from_millis(300));
        }
        std::process::exit(1);
    }
}

fn launch() -> Result<(), Box<dyn std::error::Error>> {
    let args = arguments().map_err(io::Error::other)?;
    let dir = args
        .data
        .clone()
        .or_else(|| {
            directories::ProjectDirs::from("io.github", "adhd", "adhd")
                .map(|p| p.data_local_dir().to_owned())
        })
        .ok_or("Cannot locate your user data directory")?;
    let store = Store::new(dir)?;
    if let Some(command) = args.command.as_deref() {
        if let Ok(reply) = ipc::send(&store.dir, command) {
            println!("{reply}");
            return Ok(());
        }
        if command != "show" {
            return Err("ADHD is not running. Start it with ADHD.".into());
        }
    }
    if !args.run && !args.supervise {
        if ipc::send(&store.dir, "show").is_ok() {
            return Ok(());
        }
        let mut child = Command::new(std::env::current_exe()?);
        child.arg("--supervise").arg("--data-dir").arg(&store.dir);
        if let Some(n) = args.test_interval {
            child.arg("--test-interval-seconds").arg(n.to_string());
        }
        child
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detach(&mut child);
        child.spawn()?;
        return Ok(());
    }
    if args.supervise {
        return supervise(&store, args.test_interval);
    }
    run(store, args.test_interval)
}

fn detach(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

fn supervise(store: &Store, test_interval: Option<i64>) -> Result<(), Box<dyn std::error::Error>> {
    // Cache the launch path before an installer atomically replaces the running executable.
    // On Linux current_exe() later reports an unusable " (deleted)" path for the old image.
    let executable = std::env::current_exe()?;
    let mut failures = 0u32;
    loop {
        let mut command = Command::new(&executable);
        command.arg("--run").arg("--data-dir").arg(&store.dir);
        if let Some(n) = test_interval {
            command.arg("--test-interval-seconds").arg(n.to_string());
        }
        let mut child = command.spawn()?;
        let started = Instant::now();
        let mut last_health = Instant::now();
        let mut missed_health = 0;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if last_health.elapsed() > Duration::from_secs(30) {
                last_health = Instant::now();
                if ipc::send(&store.dir, "status").is_ok() {
                    missed_health = 0
                } else {
                    missed_health += 1
                }
                if missed_health >= 3 {
                    store.log("UI did not answer three health checks; restarting.");
                    child.kill()?;
                    break child.wait()?;
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        };
        if status.success() {
            store.log("Intentional shutdown.");
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(300) {
            failures = 0;
        }
        failures += 1;
        store.log(&format!(
            "App exited unexpectedly; recovery attempt {failures}."
        ));
        if failures >= 5 {
            platform::notify("ADHD needs attention", "Repeated startup failures. Your saved goal is safe. Open ADHD to retry; diagnostic.log contains recovery details.");
            std::thread::sleep(Duration::from_secs(1));
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs((2u64.pow(failures)).min(30)));
    }
}

struct App {
    ui: Reminder,
    state: RefCell<State>,
    store: Store,
    timer: Timer,
    preview: Timer,
    draft_save: Timer,
    desktop: Arc<Mutex<Desktop>>,
    visible: Cell<bool>,
    automatic: Cell<bool>,
    test_interval: Option<i64>,
    last_clock: Cell<(i64, Instant)>,
    warned_integration: Cell<bool>,
    had_integration: Cell<bool>,
    entry_pending: Cell<bool>,
    startup: startup::Startup,
}

impl App {
    fn interval(&self) -> i64 {
        self.test_interval
            .unwrap_or_else(|| self.state.borrow().settings.interval_seconds())
    }

    fn save(&self) -> bool {
        match self.store.save(&self.state.borrow()) {
            Ok(()) => true,
            Err(_) => {
                self.ui.set_error("Your changes are still in memory, but could not be saved. Check free disk space and folder permissions.".into());
                self.store.log("Could not persist state.");
                false
            }
        }
    }

    fn refresh(&self) {
        let s = self.state.borrow();
        self.ui.set_active_goal(s.goal.is_some());
        self.ui
            .set_goal(s.goal.as_deref().unwrap_or_default().into());
        self.ui.set_prompt(
            match s.phase {
                Phase::Entry(p) => p.text(),
                _ => "Still with this?",
            }
            .into(),
        );
        self.ui.set_draft(s.draft.as_str().into());
        self.ui.set_timer_description(
            if s.settings.random {
                format!(
                    "A quiet check-in every {}–{} minutes",
                    s.settings.min_minutes, s.settings.max_minutes
                )
            } else {
                format!(
                    "A quiet check-in every {} minutes",
                    s.settings.fixed_minutes
                )
            }
            .into(),
        );
    }

    fn show(&self, manual: bool) {
        self.automatic.set(!manual);
        if !self.visible.replace(true) {
            if self.ui.window().has_winit_window() {
                self.ui.window().with_winit_window(|w| {
                    platform::prepare_window(w, manual);
                    w.set_visible(true);
                });
            }
            if self.ui.show().is_err() {
                self.visible.set(false);
                self.store.log("Could not show window.");
                platform::notify(
                    "Your ADHD reminder is ready",
                    "Open ADHD to restore your window.",
                );
                return;
            }
            Timer::single_shot(Duration::from_millis(40), move || {
                with_app(|a| {
                    a.position(false);
                    if manual {
                        a.focus();
                    }
                })
            });
        } else if manual {
            self.focus();
        }
    }

    fn focus(&self) {
        self.ui.window().with_winit_window(|w| w.focus_window());
    }

    fn conceal(&self) {
        self.preview.stop();
        if self.visible.replace(false) {
            self.remember_position();
            // Keep the native window alive so its no-focus mapping hints survive every reminder.
            self.ui.window().with_winit_window(|w| w.set_visible(false));
        }
    }

    fn remember_position(&self) {
        if let Some(position) = self
            .ui
            .window()
            .with_winit_window(|w| w.outer_position().ok())
            .flatten()
        {
            self.state.borrow_mut().position = Some((position.x, position.y));
        }
    }

    fn position(&self, reset: bool) {
        let saved = if reset {
            None
        } else {
            self.state.borrow().position
        };
        self.ui.window().with_winit_window(|w| {
            w.set_window_level(winit::window::WindowLevel::AlwaysOnTop);
            platform::prepare_window(w, !self.automatic.get());
            let monitors: Vec<_> = w.available_monitors().collect();
            let size = w.outer_size();
            let fitting = saved.and_then(|(x, y)| {
                monitors
                    .iter()
                    .find(|m| {
                        let p = m.position();
                        let s = m.size();
                        x >= p.x
                            && y >= p.y
                            && x + size.width as i32 <= p.x + s.width as i32
                            && y + size.height as i32 <= p.y + s.height as i32
                    })
                    .map(|m| (m.clone(), x, y))
            });
            let placement = fitting.or_else(|| {
                w.primary_monitor()
                    .or_else(|| monitors.first().cloned())
                    .map(|m| {
                        let p = m.position();
                        let s = m.size();
                        let margin = (24.0 * m.scale_factor()) as i32;
                        let x = (p.x + s.width as i32 - size.width as i32 - margin).max(p.x);
                        let y = p.y + (48.0 * m.scale_factor()) as i32;
                        (m, x, y)
                    })
            });
            if let Some((_, x, y)) = placement {
                w.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
            }
        });
    }

    fn start_goal(&self, text: String) {
        self.ui.set_error("".into());
        let interval = self.interval();
        let result = self.state.borrow_mut().start(&text, ipc::now(), interval);
        if let Err(error) = result {
            self.ui.set_error(error.into());
            return;
        }
        self.refresh();
        let saved = self.save();
        if saved {
            self.preview
                .start(TimerMode::SingleShot, Duration::from_secs(5), || {
                    with_app(|a| a.conceal())
                });
        }
        self.arm();
    }

    fn hide_and_reset(&self) {
        self.entry_pending.set(false);
        self.warned_integration.set(false);
        let interval = self.interval();
        self.state.borrow_mut().reset(ipc::now(), interval);
        self.ui.set_error("".into());
        self.remember_position();
        if self.save() {
            self.conceal();
        }
        self.refresh();
        self.arm();
    }

    fn finish(&self, canceled: bool) {
        self.entry_pending.set(true);
        self.preview.stop();
        self.draft_save.stop();
        self.state.borrow_mut().complete(canceled);
        self.ui.set_error("".into());
        self.save();
        self.refresh();
        self.arm();
        self.automatic.set(false);
    }

    fn arm(&self) {
        let seconds = match self.state.borrow().phase {
            Phase::Waiting { due } => (due - ipc::now()).clamp(1, 30) as u64,
            _ => 30,
        };
        self.timer
            .start(TimerMode::SingleShot, Duration::from_secs(seconds), || {
                with_app(|a| a.tick())
            });
    }

    fn tick(&self) {
        let now = ipc::now();
        let (previous, instant) = self.last_clock.replace((now, Instant::now()));
        let mut s = self.state.borrow_mut();
        let old_phase = s.phase.clone();
        s.reconcile_clock(previous, now, instant.elapsed().as_secs() as i64);
        s.due(now);
        let changed = s.phase != old_phase;
        let pending = matches!(s.phase, Phase::Pending | Phase::Entry(_));
        let desktop = self.desktop.lock().map(|d| d.clone()).unwrap_or_default();
        let may_show = may_remind(&s.settings, &desktop);
        drop(s);
        if changed {
            if matches!(self.state.borrow().phase, Phase::Pending) {
                self.preview.stop();
            }
            self.save();
        }
        self.ui.set_integration(desktop.description.as_str().into());
        if self.had_integration.replace(desktop.reliable_fullscreen) && !desktop.reliable_fullscreen
        {
            platform::notify("ADHD desktop helper disconnected", "Your goal is still saved. Full-screen detection is limited until the helper reconnects. You can always reopen ADHD.");
        }
        if self.visible.get() && self.automatic.get() && !may_show {
            self.conceal();
        }
        if pending && !self.visible.get() && may_show {
            // Hidden entry prompts are reopened only on explicit launch, not every desktop event.
            if matches!(self.state.borrow().phase, Phase::Pending) || self.entry_pending.get() {
                self.refresh();
                self.show(false);
                if self.state.borrow().goal.is_some()
                    && !desktop.reliable_fullscreen
                    && !self.warned_integration.replace(true)
                {
                    platform::notify("Your goal reminder is ready", "Open ADHD to check in. Full-screen detection is limited until your desktop helper is connected.");
                }
            }
        }
        self.arm();
    }

    fn settings(&self) {
        self.preview.stop();
        self.automatic.set(false);
        let opening = !self.ui.get_settings_open();
        self.ui.set_settings_open(opening);
        if opening {
            let s = self.state.borrow();
            self.ui.set_random_timing(s.settings.random);
            self.ui.set_min_minutes(s.settings.min_minutes as i32);
            self.ui.set_max_minutes(s.settings.max_minutes as i32);
            self.ui.set_fixed_minutes(s.settings.fixed_minutes as i32);
            self.ui.set_serious(s.settings.serious);
            self.ui.set_startup_available(self.startup.available());
            self.ui.set_startup_enabled(self.startup.enabled());
        }
        // Let Slint apply the new minimum size before requesting a smaller native window.
        Timer::single_shot(Duration::from_millis(40), || {
            with_app(|a| {
                a.ui.window().set_size(slint::LogicalSize::new(
                    420.0,
                    if a.ui.get_settings_open() {
                        560.0
                    } else {
                        380.0
                    },
                ));
                a.position(false);
            })
        });
    }

    fn save_settings(&self, settings: Settings) {
        if let Err(e) = settings.validate() {
            self.ui.set_error(e.into());
            return;
        }
        self.state.borrow_mut().settings = settings;
        let interval = self.interval();
        if matches!(self.state.borrow().phase, Phase::Waiting { .. }) {
            self.state.borrow_mut().reset(ipc::now(), interval);
        }
        self.ui.set_error("".into());
        if self.save() {
            self.settings();
        }
        self.refresh();
        self.tick();
    }

    fn command(&self, command: &str) -> String {
        match command {
            "show" => { self.preview.stop(); self.refresh(); self.show(true); "ok".into() }
            "hide" => { self.hide_and_reset(); "ok".into() }
            "quit" => { self.quit(); "ok".into() }
            "check-now" => { if self.state.borrow().goal.is_some() { self.state.borrow_mut().phase = Phase::Pending; self.save(); self.tick(); } "ok".into() }
            "status" => serde_json::json!({ "pid": std::process::id(), "visible": self.visible.get(), "phase": self.state.borrow().phase,
                "serious": self.state.borrow().settings.serious, "has_goal": self.state.borrow().goal.is_some(),
                "integration": self.ui.get_integration().as_str() }).to_string(),
            _ => "error: unknown command".into(),
        }
    }

    fn quit(&self) {
        self.remember_position();
        self.save();
        self.store.log("User requested Quit.");
        let _ = slint::quit_event_loop();
    }
}

fn run(store: Store, test_interval: Option<i64>) -> Result<(), Box<dyn std::error::Error>> {
    let Some(server) = ipc::Server::claim(&store.dir)? else {
        for _ in 0..20 {
            if ipc::send(&store.dir, "show").is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        return Err(
            "Another instance is starting but has not responded. Try ADHD again shortly.".into(),
        );
    };
    let panic_store = store.clone();
    std::panic::set_hook(Box::new(move |_| {
        panic_store.log("Unexpected panic; supervisor will recover the saved goal.");
    }));

    #[allow(unused_mut)]
    let mut event_loop: slint::winit_030::EventLoopBuilder =
        winit::event_loop::EventLoop::with_user_event();
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        event_loop.with_x11();
    }
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_event_loop_builder(event_loop)
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|a| {
            let a = a
                .with_active(false)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
            #[cfg(target_os = "linux")]
            {
                use winit::platform::x11::WindowAttributesExtX11;
                a.with_name("ADHD", "io.github.adhd.Reminder")
            }
            #[cfg(not(target_os = "linux"))]
            {
                a
            }
        })
        .select()?;
    let ui = Reminder::new()?;
    let (mut state, warning) = store.load();
    state.restore();
    let desktop = platform::watch(|_| {
        let _ = slint::invoke_from_event_loop(|| with_app(|a| a.tick()));
    });
    let app = Rc::new(App {
        ui,
        state: RefCell::new(state),
        store,
        timer: Timer::default(),
        preview: Timer::default(),
        draft_save: Timer::default(),
        desktop,
        visible: Cell::new(false),
        automatic: Cell::new(true),
        test_interval,
        last_clock: Cell::new((ipc::now(), Instant::now())),
        warned_integration: Cell::new(false),
        had_integration: Cell::new(false),
        entry_pending: Cell::new(true),
        startup: startup::Startup::new(),
    });
    APP.with(|slot| *slot.borrow_mut() = Some(app.clone()));
    app.ui
        .on_start_goal(|text| with_app(|a| a.start_goal(text.to_string())));
    app.ui.on_still_working(|| with_app(|a| a.hide_and_reset()));
    app.ui.on_finish_goal(|| with_app(|a| a.finish(false)));
    app.ui.on_cancel_goal(|| with_app(|a| a.finish(true)));
    app.ui.on_hide_card(|| with_app(|a| a.hide_and_reset()));
    app.ui.on_quit_app(|| with_app(|a| a.quit()));
    app.ui.on_open_settings(|| with_app(|a| a.settings()));
    app.ui.on_save_settings(|random, min, max, fixed, serious| {
        with_app(|a| {
            a.save_settings(Settings {
                random,
                min_minutes: min as u32,
                max_minutes: max as u32,
                fixed_minutes: fixed as u32,
                serious,
            })
        })
    });
    app.ui.on_reset_position(|| {
        with_app(|a| {
            a.state.borrow_mut().position = None;
            a.position(true);
            a.save();
        })
    });
    app.ui.on_draft_changed(|text| {
        with_app(|a| {
            a.state.borrow_mut().draft = text.chars().take(500).collect();
            a.draft_save
                .start(TimerMode::SingleShot, Duration::from_millis(500), || {
                    with_app(|a| {
                        a.save();
                    })
                });
        })
    });
    app.ui.on_set_startup(|enabled| {
        with_app(|a| {
            if let Err(e) = a.startup.set_enabled(enabled) {
                a.ui.set_error(e.into());
            }
            a.ui.set_startup_enabled(a.startup.enabled());
        })
    });
    app.ui.window().on_close_requested(|| {
        with_app(|a| a.hide_and_reset());
        slint::CloseRequestResponse::KeepWindowShown
    });
    app.ui.window().on_winit_window_event(|_, event| {
        if matches!(event, winit::event::WindowEvent::ScaleFactorChanged { .. }) {
            Timer::single_shot(Duration::from_millis(100), || {
                with_app(|a| a.position(false))
            });
        }
        slint::winit_030::EventResult::Propagate
    });
    app.refresh();
    if let Some(warning) = warning {
        app.ui.set_error(warning.into());
    }
    app.store.log("App started.");
    app.save();

    std::thread::spawn(move || {
        for incoming in server.listener.incoming() {
            let Ok(mut stream) = incoming else { break };
            let Ok(command) = server.read_command(&mut stream) else {
                continue;
            };
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            if slint::invoke_from_event_loop(move || {
                with_app(|a| {
                    let _ = send.send(a.command(&command));
                })
            })
            .is_err()
            {
                break;
            }
            if let Ok(reply) = receive.recv_timeout(Duration::from_secs(3)) {
                let _ = writeln!(stream, "{reply}");
            }
        }
    });
    // The session is already unlocked when launched manually. Startup restoration goes through tick.
    app.tick();
    slint::run_event_loop_until_quit()?;
    APP.with(|slot| slot.borrow_mut().take());
    Ok(())
}
