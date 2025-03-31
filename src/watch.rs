use crate::AppConfig;
use log::debug;
use notify::RecommendedWatcher;
use notify::Watcher;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Event {
    Reload,
    Shutdown,
}

pub(crate) type WatcherBus = Arc<RwLock<bus::Bus<Event>>>;

pub(crate) fn setup_watcher(_config: &AppConfig) -> (WatcherBus, impl Watcher) {
    let watcher_bus = Arc::new(RwLock::new(bus::Bus::new(8)));

    let watcher_bus_notify = watcher_bus.clone();
    let mut watcher = RecommendedWatcher::new(
        move |event: Result<notify::Event, notify::Error>| {
            if let Ok(event) = event {
                use notify::EventKind as Kind;
                match event.kind {
                    Kind::Remove(_) | Kind::Create(_) | Kind::Modify(_) => {
                        debug!("watcher broadcast: {:?} {:?}", event.kind, &event.paths);
                        let mut watcher_bus = watcher_bus_notify.write().unwrap();
                        watcher_bus.broadcast(Event::Reload);
                    }
                    Kind::Access(_) | Kind::Other | Kind::Any => {}
                }
            }
        },
        notify::Config::default(),
    )
    .unwrap();
    watcher
        .watch(".".as_ref(), notify::RecursiveMode::Recursive)
        .unwrap();
    debug!("watching directory: .");
    // NOTE: https://github.com/notify-rs/notify/issues/247

    (watcher_bus, watcher)
}
