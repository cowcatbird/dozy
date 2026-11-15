#[cfg(target_os = "macos")]
use crate::macos::{AwakeHold, awake_hold, idle_secs, nudge};
#[cfg(windows)]
use crate::windows::{AwakeHold, awake_hold, idle_secs, nudge};

/// Idle this long and Dozy nudges the pointer. Dozy checks at least every 30 s, so idle never reaches
/// the shortest screen saver delay (1 min) on either OS.
const NUDGE_AFTER_IDLE_SECS: f64 = 20.0;

/// Keeps the display awake and the screen saver off. Sleep holds alone don't stop the screen saver,
/// which keys on input idle time, so Dozy also nudges the pointer.
#[derive(Default)]
pub struct KeepAwake {
    hold: Option<AwakeHold>,
}

impl KeepAwake {
    pub fn sync(&mut self, on: bool) {
        if !on {
            self.hold = None;
            return;
        }
        if self.hold.is_none() {
            self.hold = awake_hold();
        }
        if self.hold.is_some() && idle_secs() >= NUDGE_AFTER_IDLE_SECS {
            nudge();
        }
    }
}
