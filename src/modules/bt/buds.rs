use crate::config::Config;
use crate::error::{FluxoError, Result as FluxoResult};
use crate::modules::bt::maestro::BudsCommand;
use crate::state::AppReceivers;
use crate::utils::TokenValue;
use futures::future::BoxFuture;

pub trait BtPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn can_handle(&self, alias: &str, mac: &str) -> bool;
    fn get_data(
        &self,
        config: &Config,
        state: &AppReceivers,
        mac: &str,
    ) -> BoxFuture<'static, FluxoResult<Vec<(String, TokenValue)>>>;
    fn get_modes(
        &self,
        mac: &str,
        state: &AppReceivers,
    ) -> BoxFuture<'static, FluxoResult<Vec<String>>>;
    fn set_mode(
        &self,
        mode: &str,
        mac: &str,
        state: &AppReceivers,
    ) -> BoxFuture<'static, FluxoResult<()>>;
    fn cycle_mode(&self, mac: &str, state: &AppReceivers) -> BoxFuture<'static, FluxoResult<()>>;
}

pub struct PixelBudsPlugin;

impl BtPlugin for PixelBudsPlugin {
    fn name(&self) -> &str {
        "Pixel Buds Pro"
    }

    fn can_handle(&self, alias: &str, _mac: &str) -> bool {
        alias.contains("Pixel Buds Pro")
    }

    fn get_data(
        &self,
        _config: &Config,
        state: &AppReceivers,
        mac: &str,
    ) -> BoxFuture<'static, FluxoResult<Vec<(String, TokenValue)>>> {
        let mac = mac.to_string();
        let state = state.clone();
        Box::pin(async move {
            let maestro = crate::modules::bt::maestro::get_maestro(&state);
            maestro.ensure_task(&mac);
            let status = maestro.get_status(&mac);

            if let Some(err) = status.error {
                return Err(FluxoError::Module {
                    module: "bt.buds",
                    message: err,
                });
            }

            let left_display = status
                .left_battery
                .map(|b| format!("{}%", b))
                .unwrap_or_else(|| "---".to_string());
            let right_display = status
                .right_battery
                .map(|b| format!("{}%", b))
                .unwrap_or_else(|| "---".to_string());

            let (anc_icon, class) = match status.anc_state.as_str() {
                "active" => ("ANC", "anc-active"),
                "aware" => ("Aware", "anc-aware"),
                "off" => ("Off", "anc-off"),
                _ => ("?", "anc-unknown"),
            };

            Ok(vec![
                ("left".to_string(), TokenValue::String(left_display)),
                ("right".to_string(), TokenValue::String(right_display)),
                ("anc".to_string(), TokenValue::String(anc_icon.to_string())),
                (
                    "plugin_class".to_string(),
                    TokenValue::String(class.to_string()),
                ),
            ])
        })
    }

    fn get_modes(
        &self,
        _mac: &str,
        _state: &AppReceivers,
    ) -> BoxFuture<'static, FluxoResult<Vec<String>>> {
        Box::pin(async move {
            Ok(vec![
                "active".to_string(),
                "aware".to_string(),
                "off".to_string(),
            ])
        })
    }

    fn set_mode(
        &self,
        mode: &str,
        mac: &str,
        state: &AppReceivers,
    ) -> BoxFuture<'static, FluxoResult<()>> {
        let mode = mode.to_string();
        let mac = mac.to_string();
        let state = state.clone();
        Box::pin(async move {
            crate::modules::bt::maestro::get_maestro(&state)
                .send_command(&mac, BudsCommand::SetAnc(mode))
                .map_err(|e: anyhow::Error| FluxoError::Module {
                    module: "bt.buds",
                    message: e.to_string(),
                })
        })
    }

    fn cycle_mode(&self, mac: &str, state: &AppReceivers) -> BoxFuture<'static, FluxoResult<()>> {
        let mac = mac.to_string();
        let state = state.clone();
        Box::pin(async move {
            let status = crate::modules::bt::maestro::get_maestro(&state).get_status(&mac);
            let next_mode = match status.anc_state.as_str() {
                "active" => "aware",
                "aware" => "off",
                _ => "active",
            };
            crate::modules::bt::maestro::get_maestro(&state)
                .send_command(&mac, BudsCommand::SetAnc(next_mode.to_string()))
                .map_err(|e: anyhow::Error| FluxoError::Module {
                    module: "bt.buds",
                    message: e.to_string(),
                })
        })
    }
}
