use anyhow::Result;
use tokio::signal;
use tokio::time::Duration;
use tracing::{error, info, warn};

use crate::config::AideConfig;
use crate::email::GmailPoller;

pub struct Daemon {
    config: AideConfig,
}

impl Daemon {
    pub fn new(config: AideConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<()> {
        info!(
            name = %self.config.aide.name,
            machines = self.config.machines.len(),
            agents = self.config.agents.len(),
            "aide daemon starting"
        );

        // Log dispatch rules
        for (task, rule) in &self.config.dispatch {
            info!(task = %task, on = %rule.on, "dispatch rule loaded");
        }

        // Log agents
        for (name, agent) in &self.config.agents {
            info!(name = %name, email = %agent.email, role = %agent.role, "agent registered");
        }

        // Start Gmail poller if credentials available
        self.start_gmail_poller();

        info!("aide daemon ready, waiting for signals");

        // Wait for shutdown signal
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("received SIGINT, shutting down");
            }
            Err(err) => {
                warn!("failed to listen for shutdown signal: {}", err);
            }
        }

        info!("aide daemon stopped");
        Ok(())
    }

    fn start_gmail_poller(&self) {
        // Try loading credentials from vault env file or environment
        let creds = self.load_gmail_creds();

        let Some((client_id, client_secret, refresh_token)) = creds else {
            warn!("Gmail credentials not found, email poller disabled");
            warn!("Set AIDE_GOOGLE_CLIENT_ID, AIDE_GOOGLE_CLIENT_SECRET, AIDE_GMAIL_REFRESH_TOKEN");
            return;
        };

        let poll_interval = Duration::from_secs(60);
        info!(interval_secs = 60, "starting Gmail poller");

        tokio::spawn(async move {
            let mut poller = GmailPoller::new(
                client_id,
                client_secret,
                refresh_token,
                poll_interval,
            );

            if let Err(e) = poller.run_poll_loop().await {
                error!(error = %e, "Gmail poller exited with error");
            }
        });
    }

    fn load_gmail_creds(&self) -> Option<(String, String, String)> {
        // Try environment variables first
        let client_id = std::env::var("AIDE_GOOGLE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("AIDE_GOOGLE_CLIENT_SECRET").ok()?;
        let refresh_token = std::env::var("AIDE_GMAIL_REFRESH_TOKEN").ok()?;
        Some((client_id, client_secret, refresh_token))
    }
}
