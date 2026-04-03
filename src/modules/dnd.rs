use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, DndState};
use futures::StreamExt;
use tokio::sync::watch;
use tracing::{debug, error, info};
use zbus::{Connection, fdo::PropertiesProxy, proxy};

pub struct DndModule;

impl WaybarModule for DndModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
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

        // Try SwayNC first
        if let Ok(proxy) = SwayncControlProxy::new(&connection).await {
            debug!("Found SwayNC, using it for DND state.");

            // Get initial state
            if let Ok(is_dnd) = proxy.dnd().await {
                let _ = tx.send(DndState { is_dnd });
            }

            // Monitor properties changed
            if let Ok(props_proxy) = PropertiesProxy::builder(&connection)
                .destination("org.erikreider.swaync.control")?
                .path("/org/erikreider/swaync/control")?
                .build()
                .await
            {
                let mut stream = props_proxy.receive_properties_changed().await?;
                while let Some(signal) = stream.next().await {
                    let args = signal.args()?;
                    if args.interface_name == "org.erikreider.swaync.control" {
                        if let Some(val) = args.changed_properties.get("dnd") {
                            if let Ok(is_dnd) = bool::try_from(val) {
                                let _ = tx.send(DndState { is_dnd });
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!("DND stream ended or daemon not found"))
    }
}
