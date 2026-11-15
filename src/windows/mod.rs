use std::ffi::c_void;
use std::ptr::null;

use windows_sys::Win32::Graphics::Gdi::{
    CreateDCW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICEW, DeleteDC, EnumDisplayDevicesW, HDC,
};
use windows_sys::Win32::UI::ColorSystem::{GetDeviceGammaRamp, SetDeviceGammaRamp};
use windows_sys::Win32::System::Power::{
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetLastInputInfo, INPUT, INPUT_0, INPUT_MOUSE, LASTINPUTINFO, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::color::Look;
use crate::engine::{Engine, Health};

type Ramp = [[u16; 256]; 3];

fn ramp(gains: [f32; 3]) -> Ramp {
    std::array::from_fn(|c| std::array::from_fn(|i| ((i as f32) * 257.0 * gains[c]).round() as u16))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Device contexts for every monitor attached to the desktop, deleted on drop.
struct Displays(Vec<HDC>);

impl Displays {
    fn attached() -> Displays {
        let mut dcs = Vec::new();
        for index in 0.. {
            let mut device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
            device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
            if unsafe { EnumDisplayDevicesW(null(), index, &mut device, 0) } == 0 {
                break;
            }
            if device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
                continue;
            }
            let dc = unsafe { CreateDCW(device.DeviceName.as_ptr(), null(), null(), null()) };
            if !dc.is_null() {
                dcs.push(dc);
            }
        }
        Displays(dcs)
    }
}

impl Drop for Displays {
    fn drop(&mut self) {
        for &dc in &self.0 {
            unsafe { DeleteDC(dc) };
        }
    }
}

/// Writes a ramp to a display unless it already has it. Returns false if Windows rejects it.
fn converge(dc: HDC, wanted: &Ramp) -> bool {
    let mut current: Ramp = [[0; 256]; 3];
    if unsafe { GetDeviceGammaRamp(dc, current.as_mut_ptr() as *mut c_void) } != 0 && current == *wanted {
        return true;
    }
    unsafe { SetDeviceGammaRamp(dc, wanted.as_ptr() as *const c_void) != 0 }
}

/// Windows refuses ramps too far from neutral unless GdiIcmGammaRange is raised.
/// When it refuses, back off toward neutral until it accepts.
const FALLBACK_STRENGTHS: [f32; 4] = [1.0, 0.75, 0.5, 0.25];

#[derive(Default)]
pub struct Gamma {
    touched: bool,
}

impl Engine for Gamma {
    fn apply(&mut self, look: Option<Look>) -> Health {
        let displays = Displays::attached();
        let Some(look) = look else {
            if self.touched {
                let neutral = ramp([1.0; 3]);
                for &dc in &displays.0 {
                    converge(dc, &neutral);
                }
                self.touched = false;
            }
            return Health::Ok;
        };
        self.touched = true;
        let gains = look.channel_gains();
        let mut limited = false;
        for &dc in &displays.0 {
            let accepted = FALLBACK_STRENGTHS.iter().position(|&t| converge(dc, &ramp(gains.map(|g| 1.0 - t * (1.0 - g)))));
            match accepted {
                Some(0) => {}
                Some(_) => limited = true,
                None => return Health::Failed("Windows rejected the gamma change".into()),
            }
        }
        if limited {
            Health::Limited("Windows is capping the warmth. Use \"Unlock warmer range\"".into())
        } else {
            Health::Ok
        }
    }
}

/// Raises Windows' gamma ramp limit so warm temperatures aren't clamped. Asks for admin through UAC.
pub fn unlock_gamma_range() {
    let params = wide(r#"add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ICM" /v GdiIcmGammaRange /t REG_DWORD /d 256 /f"#);
    unsafe {
        ShellExecuteW(std::ptr::null_mut(), wide("runas").as_ptr(), wide("reg.exe").as_ptr(), params.as_ptr(), null(), SW_HIDE);
    }
}

/// Keeps the system and display awake until dropped. Must stay on the thread that created it,
/// since the execution state belongs to the calling thread.
pub struct AwakeHold;

impl Drop for AwakeHold {
    fn drop(&mut self) {
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}

pub fn awake_hold() -> Option<AwakeHold> {
    let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) };
    (previous != 0).then_some(AwakeHold)
}

pub fn idle_secs() -> f64 {
    let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return 0.0;
    }
    // Both are 32-bit millisecond tick counts that wrap together.
    f64::from(unsafe { GetTickCount() }.wrapping_sub(info.dwTime)) / 1000.0
}

/// A zero-distance mouse move resets the idle time that starts the screen saver.
pub fn nudge() {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 } },
    };
    unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
}
