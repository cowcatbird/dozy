use std::time::{Duration, Instant};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::awake::KeepAwake;
use crate::color::Look;
use crate::engine::{self, Engine, Health};
use crate::runloop::{self, Flow, Wake, send};
use crate::settings::{EngineKind, MIN_BRIGHTNESS, Settings};

/// How often Dozy re-checks the display unprompted. macOS reports display, wake, Night Shift and
/// color filter changes as events, so this is only a safety net for changes nobody announces.
const RECHECK: Duration = Duration::from_secs(30);
const KELVIN_STEPS: [u16; 10] = [5500, 4500, 3800, 3400, 3000, 2700, 2300, 1900, 1500, 1200];
const BRIGHTNESS_STEPS: [u8; 6] = [100, 85, 70, 55, 40, 25];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    Toggle,
    Kelvin(u16),
    Warmer,
    Cooler,
    Brightness(u8),
    Engine(EngineKind),
    PauseFor(u64),
    PauseIndefinitely,
    Resume,
    LaunchAtLogin,
    KeepAwake,
    Quit,
}

impl Command {
    fn id(self) -> String {
        match self {
            Command::Toggle => "toggle".into(),
            Command::Kelvin(k) => format!("kelvin:{k}"),
            Command::Warmer => "warmer".into(),
            Command::Cooler => "cooler".into(),
            Command::Brightness(b) => format!("brightness:{b}"),
            Command::Engine(e) => format!("engine:{}", e.id()),
            Command::PauseFor(mins) => format!("pause:{mins}"),
            Command::PauseIndefinitely => "pause:forever".into(),
            Command::Resume => "resume".into(),
            Command::LaunchAtLogin => "login".into(),
            Command::KeepAwake => "awake".into(),
            Command::Quit => "quit".into(),
        }
    }

    fn parse(id: &str) -> Option<Command> {
        let (head, arg) = id.split_once(':').unwrap_or((id, ""));
        Some(match head {
            "toggle" => Command::Toggle,
            "kelvin" => Command::Kelvin(arg.parse().ok()?),
            "warmer" => Command::Warmer,
            "cooler" => Command::Cooler,
            "brightness" => Command::Brightness(arg.parse().ok()?),
            "engine" => Command::Engine(EngineKind::from_id(arg)?),
            "pause" if arg == "forever" => Command::PauseIndefinitely,
            "pause" => Command::PauseFor(arg.parse().ok()?),
            "resume" => Command::Resume,
            "login" => Command::LaunchAtLogin,
            "awake" => Command::KeepAwake,
            "quit" => Command::Quit,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Pause {
    Off,
    Until(Instant),
    Indefinitely,
}

pub enum UserEvent {
    Command(Command),
    /// The system may have undone the look.
    Recheck,
}

struct App {
    settings: Settings,
    pause: Pause,
    engine: Box<dyn Engine>,
    health: Health,
    awake: KeepAwake,
    tray: Option<TrayIcon>,
}

impl App {
    fn look(&self, now: Instant) -> Option<Look> {
        let paused = match self.pause {
            Pause::Off => false,
            Pause::Until(t) => now < t,
            Pause::Indefinitely => true,
        };
        (self.settings.enabled && !paused).then_some(Look {
            kelvin: self.settings.kelvin,
            brightness: f32::from(self.settings.brightness) / 100.0,
        })
    }

    /// The safety-net recheck, or the end of a timed pause if that comes first.
    fn next_wake(&self, now: Instant) -> Instant {
        match self.pause {
            Pause::Until(t) if t > now => t.min(now + RECHECK),
            _ => now + RECHECK,
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let pause_expired = matches!(self.pause, Pause::Until(t) if now >= t);
        if pause_expired {
            self.pause = Pause::Off;
        }
        self.awake.sync(self.settings.keep_awake);
        let health = self.engine.apply(self.look(now));
        if health != self.health || pause_expired || self.tray.is_none() {
            self.health = health;
            self.refresh_tray();
        }
    }

    /// Returns false when Dozy should quit.
    fn run(&mut self, command: Command) -> bool {
        let s = &mut self.settings;
        match command {
            Command::Toggle => {
                if self.pause != Pause::Off {
                    self.pause = Pause::Off;
                    s.enabled = true;
                } else {
                    s.enabled = !s.enabled;
                }
            }
            Command::Kelvin(k) => s.kelvin = k,
            Command::Warmer => s.kelvin = KELVIN_STEPS.into_iter().find(|&k| k < s.kelvin).unwrap_or(s.kelvin),
            Command::Cooler => s.kelvin = KELVIN_STEPS.into_iter().rev().find(|&k| k > s.kelvin).unwrap_or(s.kelvin),
            Command::Brightness(b) => s.brightness = b.clamp(MIN_BRIGHTNESS, 100),
            Command::Engine(kind) if kind != s.engine => {
                self.engine.release();
                self.engine = engine::create(kind);
                s.engine = kind;
            }
            Command::Engine(_) => {}
            Command::PauseFor(mins) => self.pause = Pause::Until(Instant::now() + Duration::from_secs(mins * 60)),
            Command::PauseIndefinitely => self.pause = Pause::Indefinitely,
            Command::Resume => self.pause = Pause::Off,
            Command::LaunchAtLogin => {
                let want = !s.launch_at_login;
                match crate::autostart::set(want) {
                    Ok(()) => s.launch_at_login = want,
                    Err(e) => self.health = Health::Failed(format!("Couldn't change launch at login: {e}")),
                }
            }
            Command::KeepAwake => s.keep_awake = !s.keep_awake,
            Command::Quit => return false,
        }
        self.settings.save();
        self.awake.sync(self.settings.keep_awake);
        self.health = self.engine.apply(self.look(Instant::now()));
        self.refresh_tray();
        true
    }

    fn refresh_tray(&mut self) {
        let warm = self.look(Instant::now()).is_some();
        let awake = self.settings.keep_awake;
        let tooltip = format!(
            "Dozy: {}\n{}\nRight-click for settings",
            if warm { format!("{} K", self.settings.kelvin) } else { "off".to_string() },
            if awake { "Keeping the screen awake. Click to stop" } else { "Click to keep the screen awake" },
        );
        match &self.tray {
            Some(tray) => {
                tray.set_menu(Some(Box::new(self.menu())));
                let _ = tray.set_icon(Some(icon(warm, awake)));
                let _ = tray.set_tooltip(Some(tooltip));
            }
            None => {
                self.tray = TrayIconBuilder::new()
                    .with_menu(Box::new(self.menu()))
                    .with_menu_on_left_click(false)
                    .with_icon(icon(warm, awake))
                    .with_tooltip(tooltip)
                    .build()
                    .ok();
            }
        }
    }

    fn menu(&self) -> Menu {
        let s = &self.settings;
        let menu = Menu::new();
        let item = |command: Command, text: &str| MenuItem::with_id(command.id(), text, true, None);
        let check = |command: Command, text: &str, checked: bool| CheckMenuItem::with_id(command.id(), text, true, checked, None);
        let note = |text: &str| MenuItem::new(text, false, None);
        let sep = PredefinedMenuItem::separator;

        let _ = menu.append(&check(Command::Toggle, "Keep screen warm", s.enabled && self.pause == Pause::Off));
        let _ = menu.append(&check(Command::KeepAwake, "Keep screen awake", s.keep_awake));
        match self.pause {
            Pause::Off => {}
            Pause::Until(t) => {
                let mins = t.saturating_duration_since(Instant::now()).as_secs().div_ceil(60);
                let _ = menu.append(&note(&format!("Paused, {mins} min left")));
            }
            Pause::Indefinitely => {
                let _ = menu.append(&note("Paused"));
            }
        }
        if self.pause != Pause::Off {
            let _ = menu.append(&item(Command::Resume, "Resume now"));
        }
        if let Health::Limited(msg) | Health::Failed(msg) = &self.health {
            let _ = menu.append(&note(&format!("⚠︎ {msg}")));
        }
        let _ = menu.append(&sep());

        let warmth = Submenu::new(format!("Warmth: {} K", s.kelvin), true);
        for k in KELVIN_STEPS {
            let _ = warmth.append(&check(Command::Kelvin(k), &format!("{k} K{}", kelvin_hint(k)), k == s.kelvin));
        }
        let _ = warmth.append(&sep());
        let _ = warmth.append(&item(Command::Warmer, "Warmer\t⌃⌥⇧↓"));
        let _ = warmth.append(&item(Command::Cooler, "Cooler\t⌃⌥⇧↑"));
        let _ = menu.append(&warmth);

        let dim = Submenu::new(format!("Brightness: {}%", s.brightness), true);
        for b in BRIGHTNESS_STEPS {
            let _ = dim.append(&check(Command::Brightness(b), &format!("{b}%"), b == s.brightness));
        }
        let _ = menu.append(&dim);

        let engines = Submenu::new("Method", true);
        for kind in EngineKind::ALL {
            let _ = engines.append(&check(Command::Engine(kind), kind.label(), kind == s.engine));
        }
        let _ = menu.append(&engines);

        let pause = Submenu::new("Pause", true);
        let _ = pause.append(&item(Command::PauseFor(15), "For 15 minutes"));
        let _ = pause.append(&item(Command::PauseFor(60), "For 1 hour"));
        let _ = pause.append(&item(Command::PauseIndefinitely, "Until I resume"));
        let _ = menu.append(&pause);

        let _ = menu.append(&sep());
        let _ = menu.append(&check(Command::LaunchAtLogin, "Launch at login", s.launch_at_login));
        let _ = menu.append(&note("Toggle: ⌃⌥⇧N"));
        let _ = menu.append(&sep());
        let _ = menu.append(&item(Command::Quit, "Quit Dozy"));
        menu
    }
}

fn kelvin_hint(k: u16) -> &'static str {
    match k {
        5500 => "  (barely warm)",
        3400 => "  (halogen)",
        2700 => "  (Night Shift max)",
        1900 => "  (candle)",
        1200 => "  (ember)",
        _ => "",
    }
}

/// The sheep as raw RGBA from tools/make-icons.py: (width, height, jumping, standing). Wide, because
/// macOS sizes menu bar icons by height.
const TRAY_ICON: (u32, u32, &[u8], &[u8]) =
    (56, 36, include_bytes!("../assets/tray-awake.rgba"), include_bytes!("../assets/tray-idle.rgba"));

/// A jumping sheep while keeping the screen awake, a standing one otherwise. Orange while warming, grey while not.
fn icon(warm: bool, awake: bool) -> Icon {
    let (width, height, jumping, standing) = TRAY_ICON;
    let mut rgba = if awake { jumping } else { standing }.to_vec();
    if !warm {
        for [r, g, b, a] in rgba.as_chunks_mut::<4>().0 {
            let luma = (u32::from(*r) * 30 + u32::from(*g) * 59 + u32::from(*b) * 11) / 100;
            let grey = (90 + luma * 45 / 100) as u8;
            (*r, *g, *b) = (grey, grey, grey);
            *a = (u32::from(*a) * 85 / 100) as u8;
        }
    }
    Icon::from_rgba(rgba, width, height).expect("icon buffer matches its size")
}

fn register_hotkeys(manager: &GlobalHotKeyManager) -> Vec<(u32, Command)> {
    let mods = Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT);
    [(Code::KeyN, Command::Toggle), (Code::ArrowDown, Command::Warmer), (Code::ArrowUp, Command::Cooler)]
        .into_iter()
        .filter_map(|(code, command)| {
            let hotkey = HotKey::new(mods, code);
            manager.register(hotkey).ok().map(|()| (hotkey.id(), command))
        })
        .collect()
}

pub fn run() {
    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        if let Some(command) = Command::parse(&event.id.0) {
            send(UserEvent::Command(command));
        }
    }));
    // Left click toggles keep awake; right click opens the menu (tray-icon's default once left click doesn't).
    TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            send(UserEvent::Command(Command::KeepAwake));
        }
    }));
    extern "C" fn recheck() {
        send(UserEvent::Recheck);
    }
    extern "C" fn quit() {
        send(UserEvent::Command(Command::Quit));
    }
    crate::macos::watch(recheck);
    crate::macos::on_terminate(quit);

    let settings = Settings::load();
    let mut app = App {
        engine: engine::create(settings.engine),
        settings,
        pause: Pause::Off,
        health: Health::Ok,
        awake: KeepAwake::default(),
        tray: None,
    };

    let hotkey_manager = GlobalHotKeyManager::new().ok();
    let hotkeys = hotkey_manager.as_ref().map(register_hotkeys).unwrap_or_default();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        let command = hotkeys.iter().find(|(id, _)| *id == event.id).map(|&(_, c)| c);
        if let (Some(command), HotKeyState::Pressed) = (command, event.state) {
            send(UserEvent::Command(command));
        }
    }));

    runloop::run(move |wake| {
        let _keep_alive = &hotkey_manager;
        match wake {
            // Start: the tray can only be created once the loop is running (required on macOS).
            Wake::Start | Wake::Timer | Wake::Event(UserEvent::Recheck) => app.tick(),
            Wake::Event(UserEvent::Command(command)) => {
                if !app.run(command) {
                    return Flow::Quit;
                }
            }
            Wake::Stop => app.engine.on_quit(),
        }
        Flow::Sleep(app.next_wake(Instant::now()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_command_survives_its_id() {
        let mut commands = vec![
            Command::Toggle,
            Command::Warmer,
            Command::Cooler,
            Command::PauseFor(15),
            Command::PauseIndefinitely,
            Command::Resume,
            Command::LaunchAtLogin,
            Command::KeepAwake,
            Command::Quit,
        ];
        commands.extend(KELVIN_STEPS.map(Command::Kelvin));
        commands.extend(BRIGHTNESS_STEPS.map(Command::Brightness));
        commands.extend(EngineKind::ALL.map(Command::Engine));
        for c in commands {
            assert_eq!(Command::parse(&c.id()), Some(c), "{}", c.id());
        }
        assert_eq!(Command::parse("kelvin:hot"), None);
    }

    #[test]
    fn a_timed_pause_wakes_dozy_when_it_ends() {
        let mut app = App {
            settings: Settings::default(),
            pause: Pause::Off,
            engine: Box::new(Inert),
            health: Health::Ok,
            awake: KeepAwake::default(),
            tray: None,
        };
        let now = Instant::now();
        assert_eq!(app.next_wake(now), now + RECHECK);

        let soon = now + Duration::from_millis(500);
        app.pause = Pause::Until(soon);
        assert_eq!(app.next_wake(now), soon);
        assert_eq!(app.look(now), None, "paused means hands off the display");
        assert!(app.look(soon).is_some(), "the look comes back when the pause ends");

        app.pause = Pause::Until(now + RECHECK * 10);
        assert_eq!(app.next_wake(now), now + RECHECK, "a long pause still gets safety rechecks");
    }

    struct Inert;
    impl Engine for Inert {
        fn apply(&mut self, _: Option<Look>) -> Health {
            Health::Ok
        }
    }
}
