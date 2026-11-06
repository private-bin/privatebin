//! Periodic removal of expired pastes.

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::state::AppState;

/// How often idle rate-limiter entries are dropped.
const PRUNE_INTERVAL: Duration = Duration::from_secs(60);

/// Background maintenance: prunes the rate limiter every minute and purges
/// expired pastes every `purge.interval_secs` (0 disables purging; pastes
/// still expire lazily on read).
pub fn spawn(state: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut prune = tokio::time::interval(PRUNE_INTERVAL);
        prune.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let secs = state.config.purge.interval_secs;
        let mut purge = (secs > 0).then(|| {
            let mut ticker = tokio::time::interval(Duration::from_secs(secs));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker
        });
        loop {
            tokio::select! {
                _ = prune.tick() => state.limiter.prune(),
                _ = async {
                    match purge.as_mut() {
                        Some(ticker) => ticker.tick().await,
                        None => std::future::pending().await,
                    }
                } => {
                    run_once(&state).await;
                }
            }
        }
    })
}

/// One purge pass. Returns the number of deleted pastes.
pub async fn run_once(state: &AppState) -> u64 {
    state.limiter.prune();
    match state.storage.purge_expired(state.now()).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!(count, "purged expired pastes");
            }
            count
        }
        Err(e) => {
            tracing::error!(error = %e, "purge failed");
            0
        }
    }
}
