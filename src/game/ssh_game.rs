//! Glue between sshhub's `SshGame` trait and stonks's central game task.
//! Owns the `mpsc::Sender`s into the central task; each new SSH session
//! authenticates against the on-disk save and forwards events.

use crate::game::agent::{DecisionAgent, UserAgent};
use crate::game::server_loop;
use crate::ssh::{
    generate_user_id, Password, AUTH_PASSWORD_SALT, AUTH_PUBLIC_KEY_SALT, MAX_USERNAME_LENGTH,
    MIN_USERNAME_LENGTH,
};
use crate::tui::Tui;
use crate::utils::{load_agent, save_agent, AgentId};
use anyhow::anyhow;
use sha2::{Digest, Sha256};
use sshhub::core::{spawn_event_converter, Credential, SshGame, SshSession, TerminalEvent};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct StonksGame {
    client_sender: mpsc::Sender<(Tui, UserAgent)>,
    terminal_event_sender: mpsc::Sender<(AgentId, TerminalEvent)>,
}

impl StonksGame {
    /// Construct the game + spawn the central market loop. Returns an
    /// `Arc<Self>` ready to hand to `sshhub::core::run_server`.
    pub fn new(reset: bool, seed: Option<u64>) -> Arc<Self> {
        let (client_sender, client_receiver) = mpsc::channel(16);
        let (terminal_event_sender, terminal_event_receiver) = mpsc::channel(64);
        server_loop::spawn(reset, seed, client_receiver, terminal_event_receiver);
        Arc::new(Self {
            client_sender,
            terminal_event_sender,
        })
    }
}

impl SshGame for StonksGame {
    type Auth = UserAgent;
    const SCREEN_SIZE: (u16, u16) = (160, 50);
    const TITLE: &'static str = "Stonks";
    const SERVER_INACTIVITY: Duration = Duration::from_secs(3600);

    async fn authenticate(
        &self,
        username: &str,
        credential: Credential,
    ) -> anyhow::Result<UserAgent> {
        let try_agent = load_agent(username);

        let username = if let Ok(a) = try_agent.as_ref() {
            a.username().to_string()
        } else if username.len() < MIN_USERNAME_LENGTH || username.len() > MAX_USERNAME_LENGTH {
            generate_user_id()
        } else {
            username.to_string()
        };

        let hashed: Password = match credential {
            Credential::Password(p) => hash_with_salt(&p, AUTH_PASSWORD_SALT),
            Credential::PublicKey(pk) => {
                // Stonks identifies pubkey users by fingerprint hash, not
                // by the openssh string - russh hands us the parsed key.
                let fp = pk.fingerprint(russh::keys::HashAlg::default());
                hash_with_salt(&fp.to_string(), AUTH_PUBLIC_KEY_SALT)
            }
        };

        if let Ok(agent) = try_agent {
            if !agent.check_password(hashed) {
                return Err(anyhow!("invalid credential for {username}"));
            }
            Ok(agent)
        } else {
            let agent = UserAgent::new(username, hashed);
            save_agent(&agent)?;
            Ok(agent)
        }
    }

    async fn on_session(self: Arc<Self>, session: SshSession<UserAgent>) {
        let SshSession {
            auth: agent,
            writer,
            data_rx,
            resize_rx,
            ..
        } = session;
        let agent_id = agent.id();

        let tui = match Tui::new(agent_id, writer) {
            Ok(t) => t,
            Err(e) => {
                log::error!("Tui init failed for {agent_id:?}: {e}");
                return;
            }
        };

        if self.client_sender.send((tui, agent)).await.is_err() {
            log::warn!("Central game task gone; dropping session for {agent_id:?}");
            return;
        }

        // Parse inbound bytes + window-changes into a single TerminalEvent
        // stream via the shared core helper, then tag each event with the
        // agent_id for the central task. Loop ends when the user
        // disconnects, at which point we drop out and the runtime closes
        // the channel.
        let mut events = spawn_event_converter(data_rx, resize_rx);
        let tev_tx = self.terminal_event_sender.clone();
        while let Some(ev) = events.recv().await {
            // Central task may already be down on shutdown; nothing to do.
            if tev_tx.send((agent_id, ev)).await.is_err() {
                break;
            }
        }
    }
}

fn hash_with_salt(input: &str, salt: &str) -> Password {
    let mut h = Sha256::new();
    h.update(format!("{input}{salt}"));
    h.finalize().into()
}
