use crate::agent::{AgentAction, DecisionAgent, UserAgent};
use crate::events::NightEvent;
use crate::market::{GamePhase, Market, HISTORICAL_SIZE, MAX_EVENTS_PER_NIGHT};
use crate::ssh_client::{Client, SessionAuth};
use crate::utils::*;
use async_trait::async_trait;
use crossterm::event::*;
use rand::seq::SliceRandom;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::Alphanumeric;
use russh::{server::*, Channel, ChannelId, Disconnect, Pty};
use russh_keys::key::PublicKey;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use strum::IntoEnumIterator;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

pub type Password = u64;
pub type AgentsDatabase = HashMap<String, UserAgent>;

const CLIENTS_DROPOUT_TIME_SECONDS: u64 = 60 * 10;
const PERSISTED_CLIENTS_DROPOUT_TIME_SECONDS: u64 = 60 * 60 * 24;
const STORE_TO_DISK_INTERVAL_SECONDS: u64 = 60;
const MARKET_TICK_INTERVAL_MILLIS: u64 = 1000;
const RENDER_INTERVAL_MILLIS: u64 = 50;
const MIN_USER_LENGTH: usize = 3;
const MAX_USER_LENGTH: usize = 16;

static AUTH_PASSWORD_SALT: &'static str = "gbasfhgE4Fvb";
static AUTH_PUBLIC_KEY_SALT: &'static str = "fa2RR4fq9XX9";

#[derive(Clone)]
pub struct AppServer {
    market: Arc<Mutex<Market>>,
    clients: Arc<Mutex<HashMap<String, Client>>>,
    agents: Arc<Mutex<AgentsDatabase>>,
    session_auth: SessionAuth,
}

impl AppServer {
    fn check_agent_password(agent: &UserAgent, password: u64) -> bool {
        agent.session_auth.hashed_password == password
    }

    fn generate_user_id() -> String {
        let buf_id = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .collect::<Vec<u8>>()
            .to_ascii_lowercase();
        std::str::from_utf8(buf_id.as_slice())
            .expect("Failed to generate user id string")
            .to_string()
    }

    pub fn new(reset: bool, seed: Option<u64>) -> AppResult<Self> {
        let market = if reset {
            info!("Creating new market from scratch");
            let mut m = Market::default();
            let rng = &mut ChaCha8Rng::seed_from_u64(
                seed.unwrap_or(ChaCha8Rng::from_entropy().next_u64()),
            );
            loop {
                m.tick_day(rng);
                if m.last_tick >= HISTORICAL_SIZE {
                    break;
                }
            }
            save_market(&m)?;
            m
        } else {
            let m = load_market().unwrap_or_default();
            info!("Loading market. Starting back from {:#?}", m.phase);
            m
        };

        let agents = if reset {
            let agents = AgentsDatabase::default();
            save_agents(&agents)?;
            agents
        } else {
            load_agents().unwrap_or_default()
        };
        info!("Loaded {} agents from store", agents.len());

        Ok(Self {
            market: Arc::new(Mutex::new(market)),
            clients: Arc::new(Mutex::new(HashMap::new())),
            agents: Arc::new(Mutex::new(agents)),
            session_auth: SessionAuth::default(),
        })
    }

    pub async fn run(&mut self, port: u16) -> AppResult<()> {
        info!("Starting SSH server. Press Ctrl-C to exit.");
        let clients = self.clients.clone();
        let agents = self.agents.clone();
        let market = self.market.clone();

        tokio::spawn(async move {
            let mut last_market_tick = SystemTime::now();
            let mut last_store_to_disk = SystemTime::now();
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(RENDER_INTERVAL_MILLIS))
                    .await;

                let mut clients = clients.lock().await;
                let mut agents = agents.lock().await;
                let mut market = market.lock().await;

                let mut character_assassination_events = vec![];
                let mut usernames = vec![];
                for stonk in market.stonks.iter() {
                    for (username, _) in stonk.shareholders.iter().take(5) {
                        if usernames.contains(&username) {
                            continue;
                        }

                        if let Some(agent) = agents.get(username) {
                            if agent
                                .past_selected_actions()
                                .contains_key(&AgentAction::AcceptBribe.to_string())
                                && !agent
                                    .past_selected_actions()
                                    .contains_key(&AgentAction::AssassinationVictim.to_string())
                            {
                                usernames.push(username);
                                character_assassination_events.push(
                                    NightEvent::CharacterAssassination {
                                        username: username.clone(),
                                    },
                                )
                            }
                        }
                    }
                }

                // If the client did not do anything recently, it wil removed.
                let mut _to_remove = vec![];
                for (id, client) in clients.iter() {
                    let try_agent = agents.get(client.username());

                    if try_agent.is_none() {
                        _to_remove.push(id.clone());
                        continue;
                    }
                    let agent = try_agent.expect("Client agent should exist in persisted agents.");

                    if agent
                        .session_auth
                        .last_active_time
                        .elapsed()
                        .expect("Time flows")
                        > Duration::from_secs(CLIENTS_DROPOUT_TIME_SECONDS)
                    {
                        _to_remove.push(id.clone());
                        continue;
                    }
                }

                clients.retain(|_, c| !_to_remove.contains(&c.username().to_string()));

                // If the client did not do anything recently, it wil removed.
                let mut _to_remove = vec![];
                for (id, client) in clients.iter_mut() {
                    let try_agent = agents.get(client.username());

                    if try_agent.is_none() {
                        _to_remove.push(id.clone());
                        continue;
                    }
                    let agent = &mut try_agent
                        .expect("Client agent should exist in persisted agents.")
                        .clone();

                    match market.phase {
                        GamePhase::Day { .. } => {
                            client.clear_render_counter();
                            agent.set_available_night_events(vec![]);
                            if let Some(_) = agent.selected_action() {
                                market
                                    .apply_agent_action::<UserAgent>(agent, &mut agents)
                                    .unwrap_or_else(|e| {
                                        error!("Could not apply agent {} action: {}", id, e)
                                    });
                            }
                        }
                        GamePhase::Night { .. } => {
                            // At the beginning of the night, set the available events.
                            // We set them here because we need the market data.
                            if client.render_counter() == 0
                                && agent.available_night_events().len() == 0
                            {
                                let mut events = NightEvent::iter()
                                    .filter(|e| {
                                        match e {
                                            NightEvent::CharacterAssassination { .. } => {
                                                return false
                                            }
                                            _ => {}
                                        };
                                        e.unlock_condition()(agent, &market)
                                    })
                                    .collect::<Vec<NightEvent>>();

                                for event in character_assassination_events.iter() {
                                    if event.unlock_condition()(agent, &market) == true {
                                        events.push(event.clone());
                                    }
                                }

                                info!("Got events {:#?}", events);
                                events.shuffle(&mut rand::thread_rng());
                                events = events
                                    .iter()
                                    .take(MAX_EVENTS_PER_NIGHT)
                                    .map(|e| e.clone())
                                    .collect::<Vec<NightEvent>>();

                                agent.set_available_night_events(events);
                            }
                            client.tick_render_counter();
                        }
                    }

                    agents.insert(agent.username().to_string(), agent.clone());
                }

                // Update market if necessary
                if last_market_tick.elapsed().expect("Time flows backwards")
                    > Duration::from_millis(MARKET_TICK_INTERVAL_MILLIS)
                {
                    market.tick();
                    last_market_tick = SystemTime::now();
                }

                // for stonk in market.stonks.iter_mut() {
                //     let allocated_shares = agents
                //         .iter()
                //         .map(|(_, agent)| agent.owned_stonks()[stonk.id])
                //         .sum::<u32>();
                //     stonk.allocated_shares = allocated_shares;
                // }

                // Draw to client TUI
                let number_of_players = clients.len();
                for (_, client) in clients.iter_mut() {
                    let agent = agents
                        .get(client.username())
                        .expect("Client agent should exist in persisted agents.");

                    client
                        .draw(&market, &agent, number_of_players)
                        .unwrap_or_else(|e| debug!("Failed to draw: {}", e));
                }

                // Store to disk
                if last_store_to_disk.elapsed().expect("Time flows backwards")
                    > Duration::from_secs(STORE_TO_DISK_INTERVAL_SECONDS)
                {
                    last_store_to_disk = SystemTime::now();
                    info!("There are {} agents", agents.len());

                    agents.retain(|_, agent| {
                        let condition = agent
                            .session_auth
                            .last_active_time
                            .elapsed()
                            .expect("Time flows")
                            <= Duration::from_secs(PERSISTED_CLIENTS_DROPOUT_TIME_SECONDS);

                        if !condition {
                            for (stonk_id, &amount) in agent.owned_stonks().iter().enumerate() {
                                let stonk = &mut market.stonks[stonk_id];
                                if let Err(e) = stonk.deallocate_shares(agent.username(), amount) {
                                    error!("Failed to deallocate share: {e}. Stonk id {stonk_id}, username {}, amount {amount}", agent.username());
                                    continue;
                                }
                            }
                        }
                        condition
                    });
                    info!("Agents: {:#?}", agents);
                    save_agents(&agents).expect("Failed to store agents to disk");
                    save_market(&market).expect("Failed to store market to disk");
                }
            }
        });

        let signing_key = load_keys().unwrap_or_else(|_| {
            let key_pair =
                russh_keys::key::KeyPair::generate_ed25519().expect("Failed to generate key pair");
            let signing_key = match key_pair {
                russh_keys::key::KeyPair::Ed25519(signing_key) => signing_key,
            };
            save_keys(&signing_key).expect("Failed to save SSH keys.");
            signing_key
        });

        let key_pair = russh_keys::key::KeyPair::Ed25519(signing_key);

        let config = Config {
            auth_rejection_time: std::time::Duration::from_secs(2),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![key_pair],
            ..Default::default()
        };

        self.run_on_address(Arc::new(config), ("0.0.0.0", port))
            .await?;

        Ok(())
    }
}

impl Server for AppServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }

    fn handle_session_error(&mut self, error: <Self::Handler as Handler>::Error) {
        error!("Session error: {}", error);
    }
}

#[async_trait]
impl Handler for AppServer {
    type Error = anyhow::Error;

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        info!("User connected with {:?}", self.session_auth);
        let mut agents = self.agents.lock().await;

        // If session_auth.username is in the persisted agents db, we check the password
        let mut agent = if let Some(db_agent) = agents.get_mut(&self.session_auth.username) {
            if Self::check_agent_password(db_agent, self.session_auth.hashed_password) == false {
                let error_string = format!("\n\rWrong password.\n");
                session.disconnect(Disconnect::ByApplication, error_string.as_str(), "");
                session.close(channel.id());
                return Ok(false);
            }
            debug!("Found existing agent in database");
            db_agent.clone()
        }
        // Else, we check the username and persist it
        else {
            if self.session_auth.username.len() < MIN_USER_LENGTH
                || self.session_auth.username.len() > MAX_USER_LENGTH
            {
                let error_string = format!(
                    "\n\rInvalid username. The username must have between {} and {} characters.\n",
                    MIN_USER_LENGTH, MAX_USER_LENGTH
                );
                session.disconnect(Disconnect::ByApplication, error_string.as_str(), "");
                session.close(channel.id());
                return Ok(false);
            }
            let new_agent = UserAgent::new(self.session_auth.clone());
            debug!("New agent created");
            new_agent
        };

        let mut clients = self.clients.lock().await;

        agent.session_auth.update_last_active_time();
        let username = agent.username().to_string();
        agents.insert(agent.username().to_string(), agent.clone());

        let try_client = Client::new(username.clone(), session.handle(), channel.id());

        if try_client.is_err() {
            let error_string = format!("\n\rFailed to create client. sorry!\n",);
            session.disconnect(Disconnect::ByApplication, error_string.as_str(), "");
            session.close(channel.id());
            return Ok(false);
        }

        debug!("Have fun {}!", username);

        clients.insert(
            username,
            try_client.expect("Client error has already been checked."),
        );

        Ok(true)
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let agents = self.agents.lock().await;
        let username = if let Some(agent) = agents.get(user) {
            agent.username().to_string()
        } else if user.len() == 0 {
            Self::generate_user_id()
        } else {
            user.to_string()
        };

        let mut hasher = DefaultHasher::new();
        let salted_password = format!("{}{}", password, AUTH_PASSWORD_SALT);
        salted_password.hash(&mut hasher);
        let hashed_password = hasher.finish();

        // We defer checking username and password to channel_open_session so that it is possible
        // to send informative error messages to the user using session.write.
        self.session_auth = SessionAuth {
            username,
            hashed_password,
            last_active_time: SystemTime::now(),
        };

        Ok(Auth::Accept)
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        debug!("Client requested public key authentication");
        let agents = self.agents.lock().await;
        let username = if let Some(agent) = agents.get(user) {
            agent.username().to_string()
        } else if user.len() == 0 {
            Self::generate_user_id()
        } else {
            user.to_string()
        };

        let mut hasher = DefaultHasher::new();
        let salted_password = format!("{}{}", public_key.fingerprint(), AUTH_PUBLIC_KEY_SALT);
        salted_password.hash(&mut hasher);
        let hashed_password = hasher.finish();

        // We defer checking username and password to channel_open_session so that it is possible
        // to send informative error messages to the user using session.write.
        self.session_auth = SessionAuth {
            username,
            hashed_password,
            last_active_time: SystemTime::now(),
        };

        Ok(Auth::Accept)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let mut clients = self.clients.lock().await;
        let number_of_players = clients.len();
        let mut end_session = false;

        if let Some(client) = clients.get_mut(&self.session_auth.username) {
            let event = convert_data_to_crossterm_event(data);
            debug!("{:?}", event);
            match event {
                Some(Event::Mouse(..)) => {}
                Some(Event::Key(key_event)) => match key_event.code {
                    KeyCode::Esc => {
                        let mut agents = self.agents.lock().await;
                        let agent = agents
                            .get_mut(client.username())
                            .expect("Agent should have been persisted");
                        agent.clear_action();
                        agent.session_auth.update_last_active_time();
                        client
                            .tui
                            .exit()
                            .await
                            .unwrap_or_else(|e| error!("Error exiting tui: {}", e));
                        end_session = true;
                    }
                    _ => {
                        let market = self.market.lock().await;
                        let mut agents = self.agents.lock().await;
                        let agent = agents
                            .get_mut(client.username())
                            .expect("Agent should have been persisted");

                        agent.session_auth.update_last_active_time();
                        client
                            .handle_key_events(key_event, &market, agent)
                            .map_err(|e| anyhow::anyhow!("Error: {}", e))?;

                        client
                            .draw(&market, &agent, number_of_players)
                            .unwrap_or_else(|e| error!("Failed to draw: {}", e));
                    }
                },
                _ => {}
            }
        } else {
            end_session = true;
        }

        if end_session {
            clients.remove(&self.session_auth.username);
            session.disconnect(Disconnect::ByApplication, "Game quit", "");
            session.close(channel);
        }

        Ok(())
    }

    // Called when the client closes a channel.
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        info!("Handling channel_close for {}", self.session_auth.username);
        let mut clients = self.clients.lock().await;
        clients.remove(&self.session_auth.username);
        session.disconnect(Disconnect::ByApplication, "Game quit", "");
        session.close(channel);

        Ok(())
    }

    /// Called when the client sends EOF to a channel.
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        info!("Handling channel_eof for {}", self.session_auth.username);
        let mut clients = self.clients.lock().await;
        clients.remove(&self.session_auth.username);
        session.disconnect(Disconnect::ByApplication, "Game quit", "");
        session.close(channel);

        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        _: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.window_change_request(
            channel, col_width, row_height, pix_width, pix_height, session,
        )
        .await
    }

    async fn window_change_request(
        &mut self,
        _: ChannelId,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Window resize request");
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get_mut(&self.session_auth.username) {
            client
                .tui
                .resize(col_width as u16, row_height as u16)
                .map_err(|e| anyhow::anyhow!("Resize error: {}", e))?;
        }
        Ok(())
    }
}
