//! Idle detection with `ext-idle-notify-v1` (Hyprland, KDE and other wlroots/KWin
//! compositors). The timeout can change at runtime; the notification is recreated.

use std::os::fd::AsFd;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1};

struct State {
    seat: Option<wl_seat::WlSeat>,
    notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    tx: Sender<bool>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => state.seat = Some(registry.bind(name, 1, qh, ())),
                "ext_idle_notifier_v1" => state.notifier = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for State {
    fn event(_: &mut Self, _: &ext_idle_notifier_v1::ExtIdleNotifierV1, _: ext_idle_notifier_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for State {
    fn event(state: &mut Self, _: &ext_idle_notification_v1::ExtIdleNotificationV1, event: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                let _ = state.tx.send(true);
            }
            ext_idle_notification_v1::Event::Resumed => {
                let _ = state.tx.send(false);
            }
            _ => {}
        }
    }
}

/// Blocks forever. `timeout_s` 0 disables notifications until it changes.
pub fn run(timeout_s: Arc<AtomicU32>, tx: Sender<bool>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("Wayland: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = State { seat: None, notifier: None, tx };
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
    let (Some(seat), Some(notifier)) = (state.seat.clone(), state.notifier.clone()) else {
        return Err("compositor ext-idle-notify-v1 sunmuyor".into());
    };
    let mut current = u32::MAX;
    let mut notification: Option<ext_idle_notification_v1::ExtIdleNotificationV1> = None;
    loop {
        let want = timeout_s.load(Ordering::Relaxed);
        if want != current {
            if let Some(n) = notification.take() {
                n.destroy();
            }
            if want > 0 {
                notification = Some(notifier.get_idle_notification(want.saturating_mul(1000), &seat, &qh, ()));
            }
            current = want;
            let _ = state.tx.send(false);
        }
        queue.dispatch_pending(&mut state).map_err(|e| e.to_string())?;
        conn.flush().map_err(|e| e.to_string())?;
        if let Some(guard) = conn.prepare_read() {
            let fd = guard.connection_fd();
            let mut fds = [nix::poll::PollFd::new(fd.as_fd(), nix::poll::PollFlags::POLLIN)];
            if nix::poll::poll(&mut fds, nix::poll::PollTimeout::from(500u16)).unwrap_or(0) > 0 {
                guard.read().map_err(|e| e.to_string())?;
            }
        }
    }
}
