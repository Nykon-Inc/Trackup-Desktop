use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Runtime};

pub struct IdleState {
    pub last_activity_timestamp: AtomicU64,
    pub is_monitoring: AtomicBool,
}

impl IdleState {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            last_activity_timestamp: AtomicU64::new(now),
            is_monitoring: AtomicBool::new(false),
        }
    }
}

pub fn start_idle_check<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    let state_listener = state.clone();
    let app_handle = app.clone();

    // 1. OS-SPECIFIC LISTENER
    thread::spawn(move || {
        run_os_listener(app_handle, state_listener);
    });
}

#[cfg(target_os = "macos")]
fn run_os_listener<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    };

    let current_loop = CFRunLoop::get_current();

    let events = vec![
        CGEventType::KeyDown,
        CGEventType::LeftMouseDown,
        CGEventType::MouseMoved,
    ];

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        events,
        move |_, _, _| {
            // Only process if monitoring
            if !state.is_monitoring.load(Ordering::Relaxed) {
                return None;
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let last = state.last_activity_timestamp.swap(now, Ordering::Relaxed);

            // Avoid logic if last was 0 or excessively old (initialization state)
            // But we initialize to `now`, so it should be fine.

            let diff = now.saturating_sub(last);
            if diff >= 60 * 5 {
                // 5 Minutes
                let app_inner = app.clone();
                // Notify main thread
                let _ = app.clone().run_on_main_thread(move || {
                    let _ = app_inner.emit("internal:idle_gap_detected", diff);
                });
            }

            None
        },
    )
    .expect("Failed to create Event Tap. Check Accessibility Permissions.");

    let source = tap
        .mach_port
        .create_runloop_source(0)
        .expect("Failed to create RunLoop source");

    unsafe {
        current_loop.add_source(&source, kCFRunLoopDefaultMode);
    }

    tap.enable();
    CFRunLoop::run_current();
}

// REAL WINDOWS LISTENER (Polling implementation is safer than hooks for a Quick Fix)
#[cfg(target_os = "windows")]
fn run_os_listener<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut last_tick = 0;

    loop {
        thread::sleep(Duration::from_millis(100));

        if !state.is_monitoring.load(Ordering::Relaxed) {
            continue;
        }

        let mut lii = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };

        unsafe {
            if GetLastInputInfo(&mut lii).as_bool() {
                let current_tick = lii.dwTime;
                if current_tick != last_tick && last_tick != 0 {
                    // Activity Detected!
                    // Calculate local gap?
                    // Actually GetLastInputInfo is system uptime.
                    // Diff between current real time and last recorded time?
                    // No, `state.last_activity_timestamp` is our own tracking.

                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let last_recorded = state.last_activity_timestamp.swap(now, Ordering::Relaxed);
                    let diff = now.saturating_sub(last_recorded);

                    if diff >= 300 {
                        let app_inner = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            let _ = app_inner.emit("internal:idle_gap_detected", diff);
                        });
                    }
                }
                last_tick = current_tick;
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_os_listener<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    // Linux X11 Polling implementation using low-level x11 crate or just shelling out?
    // Shelling out to `xprintidle` is common but depends on user installing it.
    // Pure Rust X11 polling is complicated without `x11rb` or similar.
    // I added `x11` crate.

    // Using x11 crate to query XScreenSaver.

    use std::ptr;
    use x11::xlib;
    use x11::xss;

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            eprintln!("Cannot open display for idle check");
            return;
        }
        let root = xlib::XDefaultRootWindow(display);
        let saver_info = xss::XScreenSaverAllocInfo();

        let mut last_idle_ms = 0;

        loop {
            thread::sleep(Duration::from_millis(500));
            if !state.is_monitoring.load(Ordering::Relaxed) {
                continue;
            }

            xss::XScreenSaverQueryInfo(display, root, saver_info);
            let idle_ms = (*saver_info).idle;

            // If idle_ms DROPPED significantly, it means activity happened.
            if idle_ms < last_idle_ms && last_idle_ms > 1000 {
                // Activity!
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let last_recorded = state.last_activity_timestamp.swap(now, Ordering::Relaxed);

                let diff = now.saturating_sub(last_recorded);
                if diff >= 300 {
                    let app_inner = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        let _ = app_inner.emit("internal:idle_gap_detected", diff);
                    });
                }
            }

            last_idle_ms = idle_ms;
        }

        // xlib::XCloseDisplay(display); // Loop never ends
    }
}
