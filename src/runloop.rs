//! The main-thread loop Dozy sleeps in between wakes. On macOS it is NSApplication's own run loop,
//! driven through the shim. On Windows it is tao's.

use std::time::Instant;

use crate::app::UserEvent;

pub enum Wake {
    /// The loop is running; the tray can be created now.
    Start,
    /// The time from the last `Flow::Sleep` arrived.
    Timer,
    Event(UserEvent),
    /// Dozy is about to exit. Sent once, after a handler returns `Flow::Quit`.
    Stop,
}

pub enum Flow {
    Sleep(Instant),
    Quit,
}

#[cfg(target_os = "macos")]
pub use mac::{run, send};
#[cfg(windows)]
pub use win::{run, send};

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::time::Instant;

    use super::{Flow, UserEvent, Wake};

    unsafe extern "C" {
        fn dozy_loop_run(on_start: extern "C" fn());
        fn dozy_loop_post(callback: extern "C" fn(*mut c_void), context: *mut c_void);
        fn dozy_loop_wake_in(seconds: f64, on_timer: extern "C" fn());
    }

    type Handler = Box<dyn FnMut(Wake) -> Flow>;

    thread_local! {
        static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
    }

    /// Runs `handler` on the main thread for every wake. Never returns: Dozy exits after `Wake::Stop`.
    pub fn run(handler: impl FnMut(Wake) -> Flow + 'static) {
        HANDLER.set(Some(Box::new(handler)));
        extern "C" fn on_start() {
            dispatch(Wake::Start);
        }
        unsafe { dozy_loop_run(on_start) }
    }

    /// Queues `event` for the main thread. Safe to call from any thread.
    pub fn send(event: UserEvent) {
        extern "C" fn on_post(context: *mut c_void) {
            let event = unsafe { Box::from_raw(context.cast::<UserEvent>()) };
            dispatch(Wake::Event(*event));
        }
        unsafe { dozy_loop_post(on_post, Box::into_raw(Box::new(event)).cast()) }
    }

    fn dispatch(wake: Wake) {
        let call = |wake| HANDLER.with_borrow_mut(|handler| handler.as_mut().map(|handler| handler(wake)));
        match call(wake) {
            Some(Flow::Sleep(at)) => {
                extern "C" fn on_timer() {
                    dispatch(Wake::Timer);
                }
                let seconds = at.saturating_duration_since(Instant::now()).as_secs_f64();
                unsafe { dozy_loop_wake_in(seconds, on_timer) }
            }
            Some(Flow::Quit) => {
                call(Wake::Stop);
                std::process::exit(0);
            }
            None => {}
        }
    }
}

#[cfg(windows)]
mod win {
    use std::sync::{Mutex, OnceLock};

    use tao::event::{Event, StartCause};
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};

    use super::{Flow, UserEvent, Wake};

    static PROXY: OnceLock<Mutex<EventLoopProxy<UserEvent>>> = OnceLock::new();

    /// Runs `handler` on the main thread for every wake until it returns `Flow::Quit`.
    pub fn run(mut handler: impl FnMut(Wake) -> Flow + 'static) {
        let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
        let _ = PROXY.set(Mutex::new(event_loop.create_proxy()));
        event_loop.run(move |event, _, control_flow| {
            let wake = match event {
                Event::NewEvents(StartCause::Init) => Wake::Start,
                Event::NewEvents(StartCause::ResumeTimeReached { .. }) => Wake::Timer,
                Event::UserEvent(event) => Wake::Event(event),
                Event::LoopDestroyed => {
                    handler(Wake::Stop);
                    return;
                }
                _ => return,
            };
            *control_flow = match handler(wake) {
                Flow::Sleep(at) => ControlFlow::WaitUntil(at),
                Flow::Quit => ControlFlow::Exit,
            };
        });
    }

    /// Queues `event` for the main thread. Safe to call from any thread.
    pub fn send(event: UserEvent) {
        if let Some(proxy) = PROXY.get() {
            let _ = proxy.lock().unwrap().send_event(event);
        }
    }
}
