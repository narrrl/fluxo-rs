use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, DndState};
use futures::StreamExt;
use tokio::sync::watch;
use tokio::time::Duration;
use tracing::{debug, error, info};
use zbus::{Connection, fdo::PropertiesProxy, names::InterfaceName, proxy};

pub struct DndModule;

impl WaybarModule for DndModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        args: &[&str],
    ) -> Result<WaybarOutput> {
        let action = args.first().unwrap_or(&"show");

        if *action == "toggle" {
            let connection =
                Connection::session()
                    .await
                    .map_err(|e| crate::error::FluxoError::Module {
                        module: "dnd",
                        message: format!("DBus connection failed: {}", e),
                    })?;

            // Try toggling SwayNC
            if let Ok(proxy) = SwayncControlProxy::new(&connection).await
                && let Ok(is_dnd) = proxy.dnd().await
            {
                let _ = proxy.set_dnd(!is_dnd).await;
                return Ok(WaybarOutput::default());
            }

            // Try toggling Dunst
            if let Ok(proxy) = DunstControlProxy::new(&connection).await
                && let Ok(is_paused) = proxy.paused().await
            {
                let _ = proxy.set_paused(!is_paused).await;
                return Ok(WaybarOutput::default());
            }

            return Err(crate::error::FluxoError::Module {
                module: "dnd",
                message: "No supported notification daemon found to toggle".to_string(),
            });
        }

        let is_dnd = state.dnd.borrow().is_dnd;

        if is_dnd {
            Ok(WaybarOutput {
                text: config.dnd.format_dnd.clone(),
                tooltip: Some("Do Not Disturb: On".to_string()),
                class: Some("dnd".to_string()),
                percentage: None,
            })
        } else {
            Ok(WaybarOutput {
                text: config.dnd.format_normal.clone(),
                tooltip: Some("Do Not Disturb: Off".to_string()),
                class: Some("normal".to_string()),
                percentage: None,
            })
        }
    }
}

pub struct DndDaemon;

#[proxy(
    interface = "org.erikreider.swaync.control",
    default_service = "org.erikreider.swaync.control",
    default_path = "/org/erikreider/swaync/control"
)]
trait SwayncControl {
    #[zbus(property)]
    fn dnd(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_dnd(&self, value: bool) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.dunstproject.cmd0",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait DunstControl {
    #[zbus(property)]
    fn paused(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_paused(&self, value: bool) -> zbus::Result<()>;
}

impl DndDaemon {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self, tx: watch::Sender<DndState>) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = Self::listen_loop(&tx).await {
                    error!("DND listener error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        });
    }

    async fn listen_loop(tx: &watch::Sender<DndState>) -> anyhow::Result<()> {
        let connection = Connection::session().await?;

        info!("Connected to D-Bus for DND monitoring");

        // Try SwayNC first (signal-based)
        if let Ok(proxy) = SwayncControlProxy::new(&connection).await
            && let Ok(is_dnd) = proxy.dnd().await
        {
            debug!("Found SwayNC, using signal-based DND monitoring");
            let _ = tx.send(DndState { is_dnd });

            if let Ok(props_proxy) = PropertiesProxy::builder(&connection)
                .destination("org.erikreider.swaync.control")?
                .path("/org/erikreider/swaync/control")?
                .build()
                .await
            {
                let mut stream = props_proxy.receive_properties_changed().await?;
                while let Some(signal) = stream.next().await {
                    let args = signal.args()?;
                    if args.interface_name == "org.erikreider.swaync.control"
                        && let Some(val) = args.changed_properties.get("dnd")
                        && let Ok(is_dnd) = bool::try_from(val)
                    {
                        let _ = tx.send(DndState { is_dnd });
                    }
                }
            }

            return Err(anyhow::anyhow!("SwayNC DND stream ended"));
        }

        // Try Dunst (polling via raw D-Bus Properties.Get for maximum compatibility)
        let props_proxy = PropertiesProxy::builder(&connection)
            .destination("org.freedesktop.Notifications")?
            .path("/org/freedesktop/Notifications")?
            .build()
            .await;

        match &props_proxy {
            Ok(_) => debug!("Created Properties proxy for Dunst"),
            Err(e) => debug!("Failed to create Properties proxy for Dunst: {}", e),
        }

        if let Ok(props) = props_proxy {
            // Read paused property via raw Properties.Get
            let initial = props
                .get(
                    InterfaceName::from_static_str_unchecked("org.dunstproject.cmd0"),
                    "paused",
                )
                .await;

            match &initial {
                Ok(_) => debug!("Successfully read Dunst paused property"),
                Err(e) => debug!("Failed to read Dunst paused property: {}", e),
            }

            if let Ok(value) = initial
                && let Ok(is_paused) = bool::try_from(&*value)
            {
                info!("Found Dunst, using polling-based DND monitoring");
                let _ = tx.send(DndState { is_dnd: is_paused });

                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    match props
                        .get(
                            InterfaceName::from_static_str_unchecked("org.dunstproject.cmd0"),
                            "paused",
                        )
                        .await
                    {
                        Ok(value) => {
                            if let Ok(is_paused) = bool::try_from(&*value) {
                                let current = tx.borrow().is_dnd;
                                if current != is_paused {
                                    let _ = tx.send(DndState { is_dnd: is_paused });
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Dunst paused() poll failed: {}", e);
                            break;
                        }
                    }
                }

                return Err(anyhow::anyhow!("Dunst connection lost"));
            }
        }

        Err(anyhow::anyhow!(
            "No supported notification daemon found (tried SwayNC, Dunst)"
        ))
    }
}
