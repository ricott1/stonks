use super::channel::AppChannel;
use super::SSHEventHandler;
use super::SSHWriterProxy;
use super::TerminalEvent;
use crate::game::agent::DecisionAgent;
use crate::game::agent::UserAgent;
use crate::ssh::utils::generate_user_id;
use crate::tui::Tui;
use crate::utils::*;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use log::debug;
use russh::Channel;
use russh::Pty;
use russh::{server::*, ChannelId};
use russh_keys::HashAlg;
use russh_keys::PublicKey;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

static AUTH_PASSWORD_SALT: &'static str = "gbasfhgE4Fvb";
static AUTH_PUBLIC_KEY_SALT: &'static str = "fa2RR4fq9XX9";

const MIN_USERNAME_LENGTH: usize = 3;
const MAX_USERNAME_LENGTH: usize = 12;

pub struct AppClient {
    id: AgentId,
    agent: Option<UserAgent>,
    client_sender: Sender<(Tui, UserAgent)>,
    terminal_event_sender: Sender<(AgentId, TerminalEvent)>,
    server_shutdown: CancellationToken,
    channels: HashMap<ChannelId, AppChannel>,
}

impl AppClient {
    pub fn new(
        server_shutdown: CancellationToken,
        client_sender: Sender<(Tui, UserAgent)>,
        terminal_event_sender: Sender<(AgentId, TerminalEvent)>,
    ) -> Self {
        AppClient {
            id: AgentId::default(),
            agent: None,
            client_sender,
            terminal_event_sender,
            server_shutdown,
            channels: HashMap::new(),
        }
    }

    fn channel_mut(&mut self, id: ChannelId) -> AppResult<&mut AppChannel> {
        self.channels
            .get_mut(&id)
            .with_context(|| format!("unknown channel: {}", id))
    }
}

#[async_trait]
impl Handler for AppClient {
    type Error = anyhow::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> AppResult<Auth> {
        debug!("Client requested password authentication");
        println!("Trying to load agent {}", user);
        let try_agent = load_agent(user);

        let username = if let Ok(agent) = try_agent.as_ref() {
            agent.username().to_string()
        } else if user.len() < MIN_USERNAME_LENGTH || user.len() > MAX_USERNAME_LENGTH {
            generate_user_id()
        } else {
            user.to_string()
        };

        let mut hasher = Sha256::new();
        let salted_password = format!("{}{}", password, AUTH_PASSWORD_SALT);
        hasher.update(salted_password);
        let hashed_password = hasher.finalize().to_vec()[..].try_into()?;

        let agent = if let Ok(agent) = try_agent {
            if agent.check_password(hashed_password) == false {
                return Err(anyhow!("Invalid password for user {}", username));
            }
            agent
        } else {
            let agent = UserAgent::new(username.to_string(), hashed_password);
            println!("Saving agent {}", username);
            save_agent(&agent)?;
            agent
        };

        self.agent = Some(agent);

        Ok(Auth::Accept)
    }

    async fn auth_publickey(&mut self, user: &str, public_key: &PublicKey) -> AppResult<Auth> {
        debug!("Client requested public key authentication");
        let try_agent = load_agent(user);

        let username = if let Ok(agent) = try_agent.as_ref() {
            agent.username().to_string()
        } else if user.len() < MIN_USERNAME_LENGTH || user.len() > MAX_USERNAME_LENGTH {
            generate_user_id()
        } else {
            user.to_string()
        };

        let mut hasher = Sha256::new();
        let salted_password = format!(
            "{}{}",
            public_key.fingerprint(HashAlg::default()),
            AUTH_PUBLIC_KEY_SALT
        );
        hasher.update(salted_password);
        let hashed_password = hasher.finalize().to_vec()[..].try_into()?;

        let agent = if let Ok(agent) = try_agent {
            if agent.check_password(hashed_password) == false {
                return Err(anyhow!("Invalid key for user {}", username));
            }
            agent
        } else {
            let agent = UserAgent::new(username.to_string(), hashed_password);
            println!("Saving agent {}", username);
            save_agent(&agent)?;
            agent
        };

        self.id = agent.id();
        self.agent = Some(agent);

        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> AppResult<bool> {
        println!("Client requested a new session channel");
        let app_channel = AppChannel::new();
        let created = self.channels.insert(channel.id(), app_channel).is_none();
        assert!(self.agent.is_some());

        if created {
            Ok(true)
        } else {
            Err(anyhow!(
                "channel `{}` has been already opened",
                channel.id()
            ))
        }
    }

    async fn channel_close(&mut self, channel: ChannelId, _: &mut Session) -> AppResult<()> {
        if self.channels.remove(&channel).is_some() {
            Ok(())
        } else {
            Err(anyhow!("channel `{}` has been already closed", channel))
        }
    }

    async fn data(&mut self, id: ChannelId, data: &[u8], _: &mut Session) -> AppResult<()> {
        self.channel_mut(id)?.data(data).await?;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel_id: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut Session,
    ) -> AppResult<()> {
        println!("Client requested pty");
        let stdin = self.channel_mut(channel_id)?.pty_request().await?;
        let client_shutdown = CancellationToken::new();

        SSHEventHandler::start(
            stdin,
            self.terminal_event_sender.clone(),
            self.id,
            client_shutdown,
            self.server_shutdown.clone(),
        );
        let writer = SSHWriterProxy::new(channel_id, session.handle());
        let tui = Tui::new(self.id, writer)?;

        self.client_sender
            .send((tui, self.agent.take().unwrap()))
            .await?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        id: ChannelId,
        width: u32,
        height: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> AppResult<()> {
        self.channel_mut(id)?
            .window_change_request(width, height)
            .await?;

        Ok(())
    }
}
