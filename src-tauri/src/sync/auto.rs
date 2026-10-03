use crate::sync::SyncService;
use std::{sync::Arc, time::Duration};
use tokio::time::{interval, sleep, MissedTickBehavior};

pub fn spawn(service: SyncService, interval_fn: Arc<dyn Fn() -> Option<Duration> + Send + Sync>) {
    let periodic = service.clone();
    let periodic_interval = interval_fn.clone();
    tokio::spawn(async move {
        loop {
            let Some(duration) = periodic_interval() else {
                sleep(Duration::from_secs(5)).await;
                continue;
            };
            let mut ticker = interval(duration);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            ticker.tick().await;
            ticker.tick().await;
            let _ = periodic.sync_now().await;
        }
    });

    tokio::spawn(async move {
        let mut changes = service.subscribe();
        loop {
            match changes.recv().await {
                Ok(event) if !event.remote => {
                    sleep(Duration::from_secs(2)).await;
                    while let Ok(event) = changes.try_recv() {
                        if event.remote {
                            continue;
                        }
                        sleep(Duration::from_secs(2)).await;
                    }
                    let _ = service.sync_now().await;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
