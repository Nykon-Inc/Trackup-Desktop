use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Runtime};

pub struct IdleState {
    pub last_activity_timestamp: AtomicU64,
    pub is_monitoring: AtomicBool,
    pub keyboard_count: AtomicU64,
    pub mouse_count: AtomicU64,
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
            keyboard_count: AtomicU64::new(0),
            mouse_count: AtomicU64::new(0),
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
        move |_proxy, type_, _event| {
            // Only process if monitoring
            if !state.is_monitoring.load(Ordering::Relaxed) {
                return None;
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let last = state.last_activity_timestamp.swap(now, Ordering::Relaxed);

            // Simple counting since we have event type
            match type_ {
                CGEventType::KeyDown => {
                    state.keyboard_count.fetch_add(1, Ordering::Relaxed);
                }
                CGEventType::LeftMouseDown | CGEventType::MouseMoved => {
                    state.mouse_count.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }

            let diff = now.saturating_sub(last);
            if diff >= 300 {
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
        thread::sleep(std::time::Duration::from_millis(100));

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
                    // On Windows, GetLastInputInfo doesn't tell us KEY vs MOUSE.
                    // We will just increment mouse_count as a generic "activity" counter for now
                    // or split it evenly? Let's just do mouse_count to be safe/lazy or maybe both?
                    // Better: just increment mouse_count.
                    state.mouse_count.fetch_add(1, Ordering::Relaxed);

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
            thread::sleep(std::time::Duration::from_millis(500));
            if !state.is_monitoring.load(Ordering::Relaxed) {
                continue;
            }

            xss::XScreenSaverQueryInfo(display, root, saver_info);
            let idle_ms = (*saver_info).idle;

            // If idle_ms DROPPED significantly, it means activity happened.
            if idle_ms < last_idle_ms && last_idle_ms > 1000 {
                // Activity!
                state.mouse_count.fetch_add(1, Ordering::Relaxed);

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
    }
}
