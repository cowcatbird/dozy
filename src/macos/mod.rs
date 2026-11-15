use std::ffi::c_void;

use crate::color::Look;
use crate::engine::{Engine, Health};

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct NightShiftState {
    available: bool,
    enabled: bool,
    strength: f32,
    min_kelvin: f32,
    max_kelvin: f32,
}

#[repr(C)]
#[derive(Default, Clone, Copy, PartialEq, Debug)]
struct FilterState {
    enabled: bool,
    kind: i64,
    hue: f64,
    intensity: f64,
}

unsafe extern "C" {
    fn dozy_ns_read(out: *mut NightShiftState) -> bool;
    fn dozy_ns_set_enabled(enabled: bool) -> bool;
    fn dozy_ns_set_strength(strength: f32) -> bool;
    fn dozy_displays(ids: *mut u32, max: u32) -> u32;
    fn dozy_gamma_get(display: u32, gains: *mut f32) -> bool;
    fn dozy_gamma_set(display: u32, gains: *const f32) -> bool;
    fn dozy_gamma_reset();
    fn dozy_filter_read(out: *mut FilterState) -> bool;
    fn dozy_filter_write(state: *const FilterState) -> bool;
    fn dozy_awake_hold() -> *mut c_void;
    fn dozy_awake_release(token: *mut c_void);
    fn dozy_idle_secs() -> f64;
    fn dozy_nudge();
    fn dozy_watch(callback: extern "C" fn());
    fn dozy_on_terminate(callback: extern "C" fn());
}

/// Calls `on_change` whenever the system may have undone Dozy's look. See `dozy_watch` in shim.m.
pub fn watch(on_change: extern "C" fn()) {
    unsafe { dozy_watch(on_change) }
}

/// Calls `on_quit` on the main thread for kill, logout and Ctrl-C instead of dying on the spot.
pub fn on_terminate(on_quit: extern "C" fn()) {
    unsafe { dozy_on_terminate(on_quit) }
}

fn read_night_shift() -> Option<NightShiftState> {
    let mut s = NightShiftState::default();
    unsafe { dozy_ns_read(&mut s) }.then_some(s)
}

/// Keeps macOS Night Shift switched on and at Dozy's strength, whatever its own schedule says.
pub struct NightShift {
    original: Option<NightShiftState>,
}

impl NightShift {
    pub fn new() -> NightShift {
        NightShift { original: read_night_shift() }
    }
}

impl Engine for NightShift {
    fn apply(&mut self, look: Option<Look>) -> Health {
        let Some(now) = read_night_shift() else {
            return Health::Failed("Couldn't reach Night Shift on this Mac".into());
        };
        if !now.available {
            return Health::Failed("This Mac doesn't support Night Shift".into());
        }
        let Some(look) = look else {
            if now.enabled {
                unsafe { dozy_ns_set_enabled(false) };
            }
            return Health::Ok;
        };

        let span = now.max_kelvin - now.min_kelvin;
        let strength = ((now.max_kelvin - f32::from(look.kelvin)) / span).clamp(0.0, 1.0);
        if (now.strength - strength).abs() > 0.005 {
            unsafe { dozy_ns_set_strength(strength) };
        }
        if !now.enabled {
            unsafe { dozy_ns_set_enabled(true) };
        }

        if f32::from(look.kelvin) < now.min_kelvin {
            Health::Limited(format!("Night Shift stops at {} K. Switch to Gamma to go warmer", now.min_kelvin as u16))
        } else if look.brightness < 1.0 {
            Health::Limited("Night Shift can't dim. Switch to Gamma to dim".into())
        } else {
            Health::Ok
        }
    }

    fn release(&mut self) {
        if let Some(original) = self.original {
            unsafe {
                dozy_ns_set_strength(original.strength);
                dozy_ns_set_enabled(original.enabled);
            }
        }
    }

    /// Quitting a keeper shouldn't plunge the screen back to blue, so Night Shift is left as it is.
    fn on_quit(&mut self) {}
}

/// Scales each display's gamma curve. macOS resets these on wake and display changes,
/// and the per-tick `apply` puts them back.
#[derive(Default)]
pub struct Gamma {
    touched: bool,
}

impl Engine for Gamma {
    fn apply(&mut self, look: Option<Look>) -> Health {
        let Some(look) = look else {
            if self.touched {
                unsafe { dozy_gamma_reset() };
                self.touched = false;
            }
            return Health::Ok;
        };
        let gains = look.channel_gains();
        let mut ids = [0u32; 16];
        let count = unsafe { dozy_displays(ids.as_mut_ptr(), ids.len() as u32) } as usize;
        let mut failed = 0;
        for &display in &ids[..count] {
            let mut current = [0f32; 3];
            let matches = unsafe { dozy_gamma_get(display, current.as_mut_ptr()) }
                && current.iter().zip(gains).all(|(c, g)| (c - g).abs() < 0.002);
            if !matches && !unsafe { dozy_gamma_set(display, gains.as_ptr()) } {
                failed += 1;
            }
        }
        self.touched = true;
        match failed {
            0 => Health::Ok,
            n => Health::Failed(format!("{n} display(s) refused the gamma change")),
        }
    }
}

/// Dozy's tint is the Accessibility "Color Tint" filter at an orange hue.
const COLOR_TINT: i64 = 16;
const DOZY_HUE: f64 = 0.083;
const MIN_INTENSITY: f64 = 0.25;
const MAX_INTENSITY: f64 = 0.75;

fn read_filter() -> Option<FilterState> {
    let mut s = FilterState::default();
    unsafe { dozy_filter_read(&mut s) }.then_some(s)
}

/// Drives the system-wide Accessibility color filter, which is applied after everything else on screen.
pub struct ColorFilter {
    /// The user's own filter settings, put back when Dozy lets go.
    original: FilterState,
    active: bool,
}

impl ColorFilter {
    pub fn new() -> ColorFilter {
        let mut original = read_filter().unwrap_or_default();
        // A previous run that didn't get to clean up leaves Dozy's own tint behind. Don't treat it as the user's.
        if original.kind == COLOR_TINT && (original.hue - DOZY_HUE).abs() < 1e-6 {
            original.enabled = false;
        }
        ColorFilter { original, active: false }
    }
}

impl Engine for ColorFilter {
    fn apply(&mut self, look: Option<Look>) -> Health {
        let Some(current) = read_filter() else {
            return Health::Failed("Color filters aren't available on this macOS version".into());
        };
        let Some(look) = look.filter(|l| l.warmth() > 0.0) else {
            if self.active {
                unsafe { dozy_filter_write(&self.original) };
                self.active = false;
            }
            return Health::Ok;
        };
        let target = FilterState {
            enabled: true,
            kind: COLOR_TINT,
            hue: DOZY_HUE,
            intensity: MIN_INTENSITY + (MAX_INTENSITY - MIN_INTENSITY) * f64::from(look.warmth()),
        };
        let same = current.enabled
            && current.kind == target.kind
            && (current.hue - target.hue).abs() < 1e-6
            && (current.intensity - target.intensity).abs() < 1e-3;
        if !same {
            unsafe { dozy_filter_write(&target) };
        }
        self.active = true;
        if look.brightness < 1.0 {
            Health::Limited("Color Filter can't dim. Switch to Gamma to dim".into())
        } else {
            Health::Ok
        }
    }
}

/// Night Shift would stack on top of another engine's tint, so it's held off while that engine runs
/// and switched back on when Dozy lets go.
pub struct NightShiftHeldOff<E> {
    inner: E,
    was_enabled: Option<bool>,
}

impl<E> NightShiftHeldOff<E> {
    pub fn new(inner: E) -> Self {
        NightShiftHeldOff { inner, was_enabled: None }
    }
}

impl<E: Engine> Engine for NightShiftHeldOff<E> {
    fn apply(&mut self, look: Option<Look>) -> Health {
        if let Some(ns) = read_night_shift() {
            self.was_enabled.get_or_insert(ns.enabled);
            if ns.enabled {
                unsafe { dozy_ns_set_enabled(false) };
            }
        }
        self.inner.apply(look)
    }

    fn release(&mut self) {
        self.inner.release();
        if self.was_enabled.take() == Some(true) {
            unsafe { dozy_ns_set_enabled(true) };
        }
    }
}

/// An open power-management activity that keeps the display from sleeping. Released on drop.
pub struct AwakeHold(*mut c_void);

impl Drop for AwakeHold {
    fn drop(&mut self) {
        unsafe { dozy_awake_release(self.0) }
    }
}

pub fn awake_hold() -> Option<AwakeHold> {
    let token = unsafe { dozy_awake_hold() };
    (!token.is_null()).then_some(AwakeHold(token))
}

pub fn idle_secs() -> f64 {
    unsafe { dozy_idle_secs() }
}

/// A zero-distance mouse move resets the HID idle time, which is what starts the screen saver.
/// Sleep assertions don't touch it.
pub fn nudge() {
    unsafe { dozy_nudge() }
}
