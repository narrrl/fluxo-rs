use crate::output::WaybarOutput;
use crate::state::{AppReceivers, ModuleHealth};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Check if a module is in backoff (used by request handler).
pub async fn check_backoff(
    module_name: &str,
    state: &AppReceivers,
) -> (bool, Option<WaybarOutput>) {
    let lock = state.health.read().await;
    if let Some(health) = lock.get(module_name) {
        let in_backoff = health
            .backoff_until
            .is_some_and(|until| Instant::now() < until);
        (in_backoff, health.last_successful_output.clone())
    } else {
        (false, None)
    }
}

/// Update health after a request dispatch (used by request handler).
pub async fn update_health(
    module_name: &str,
    result: &Result<WaybarOutput, crate::error::FluxoError>,
    state: &AppReceivers,
) {
    let mut lock = state.health.write().await;
    let health = lock.entry(module_name.to_string()).or_default();
    match result {
        Ok(output) => {
            health.consecutive_failures = 0;
            health.backoff_until = None;
            health.last_successful_output = Some(output.clone());
        }
        Err(e) => {
            health.consecutive_failures += 1;
            health.last_failure = Some(Instant::now());
            if health.consecutive_failures >= 3 {
                health.backoff_until = Some(Instant::now() + Duration::from_secs(30));
                warn!(module = module_name, error = %e, "Module entered backoff state due to repeated failures");
            }
        }
    }
}

/// Check if a polling daemon module is in backoff.
pub async fn is_poll_in_backoff(
    module_name: &str,
    health_lock: &Arc<RwLock<HashMap<String, ModuleHealth>>>,
) -> bool {
    let lock = health_lock.read().await;
    if let Some(health) = lock.get(module_name)
        && let Some(until) = health.backoff_until
    {
        return Instant::now() < until;
    }
    false
}

/// Update health after a polling daemon result.
pub async fn handle_poll_result(
    module_name: &str,
    result: crate::error::Result<()>,
    health_lock: &Arc<RwLock<HashMap<String, ModuleHealth>>>,
) {
    let mut lock = health_lock.write().await;
    let health = lock.entry(module_name.to_string()).or_default();

    match result {
        Ok(_) => {
            if health.consecutive_failures > 0 {
                info!(
                    module = module_name,
                    "Module recovered after {} failures", health.consecutive_failures
                );
            }
            health.consecutive_failures = 0;
            health.backoff_until = None;
        }
        Err(e) => {
            health.consecutive_failures += 1;
            health.last_failure = Some(Instant::now());

            if !e.is_transient() {
                health.backoff_until = Some(Instant::now() + Duration::from_secs(60));
                error!(module = module_name, error = %e, "Fatal module error, entering long cooldown");
            } else if health.consecutive_failures >= 3 {
                let backoff_secs = 30 * (2u64.pow(health.consecutive_failures.saturating_sub(3)));
                let backoff_secs = backoff_secs.min(3600);
                health.backoff_until = Some(Instant::now() + Duration::from_secs(backoff_secs));
                warn!(module = module_name, error = %e, backoff = backoff_secs, "Repeated transient failures, entering backoff");
            } else {
                debug!(module = module_name, error = %e, "Transient module failure (attempt {})", health.consecutive_failures);
            }
        }
    }
}

pub fn backoff_response(module_name: &str, cached: Option<WaybarOutput>) -> String {
    if let Some(mut cached) = cached {
        let class = cached.class.unwrap_or_default();
        cached.class = Some(format!("{} warning", class).trim().to_string());
        return serde_json::to_string(&cached).unwrap_or_else(|_| "{}".to_string());
    }
    format!(
        "{{\"text\":\"\u{200B}Cooling down ({})\u{200B}\",\"class\":\"error\"}}",
        module_name
    )
}

pub fn error_response(
    module_name: &str,
    e: &crate::error::FluxoError,
    cached: Option<WaybarOutput>,
) -> String {
    if let Some(mut cached) = cached {
        let class = cached.class.unwrap_or_default();
        cached.class = Some(format!("{} warning", class).trim().to_string());
        return serde_json::to_string(&cached).unwrap_or_else(|_| "{}".to_string());
    }

    let error_msg = e.to_string();
    error!(module = module_name, error = %error_msg, "Module execution failed");
    let err_out = WaybarOutput {
        text: "\u{200B}Error\u{200B}".to_string(),
        tooltip: Some(error_msg),
        class: Some("error".to_string()),
        percentage: None,
    };
    serde_json::to_string(&err_out).unwrap_or_else(|_| "{}".to_string())
}
