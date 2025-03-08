use crate::AppConfig;
use log::debug;
use notify::RecommendedWatcher;
use notify::Watcher;
use std::sync::{Arc, RwLock};

pub(crate) type WatcherBus = Arc<RwLock<bus::Bus<notify::Event>>>;

pub(crate) fn setup_watcher(config: &AppConfig) -> WatcherBus {
    let watcher_bus = Arc::new(RwLock::new(bus::Bus::new(8)));

    let watcher_bus_notify = watcher_bus.clone();

    if config.enable_reload {
        let mut watcher = RecommendedWatcher::new(
            move |event: Result<notify::Event, notify::Error>| {
                if let Ok(event) = event {
                    use notify::EventKind as Kind;
                    match event.kind {
                        Kind::Remove(_) | Kind::Create(_) | Kind::Modify(_) => {
                            debug!("watcher broadcast: {:?} {:?}", event.kind, &event.paths);
                            let mut watcher_bus = watcher_bus_notify.write().unwrap();
                            watcher_bus.broadcast(event);
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
    }
    //FIXME: return watcher so it's not drop()ed LOL

    watcher_bus
}
