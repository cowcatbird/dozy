use crate::color::Look;
use crate::settings::EngineKind;

/// Result of one `apply`, shown as a note in the menu when it isn't `Ok`.
#[derive(Clone, PartialEq, Debug)]
pub enum Health {
    Ok,
    /// Applied, but the engine couldn't reach the full look.
    Limited(String),
    Failed(String),
}

pub trait Engine {
    /// Converge the display on `look`, or on the untouched system look for `None`.
    /// Runs every tick, so it must be cheap and change nothing when the display already matches.
    fn apply(&mut self, look: Option<Look>) -> Health;

    /// Hand the display back to the system. Called when switching engines.
    fn release(&mut self) {
        self.apply(None);
    }

    /// Called when Dozy quits.
    fn on_quit(&mut self) {
        self.release();
    }
}

pub fn create(kind: EngineKind) -> Box<dyn Engine> {
    match kind {
        EngineKind::NightShift => Box::new(crate::macos::NightShift::new()),
        EngineKind::Gamma => Box::new(crate::macos::NightShiftHeldOff::new(crate::macos::Gamma::default())),
        EngineKind::ColorFilter => Box::new(crate::macos::NightShiftHeldOff::new(crate::macos::ColorFilter::new())),
    }
}
