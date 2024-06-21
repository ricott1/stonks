use crate::agent::{AgentAction, DecisionAgent, NightEvent, UserAgent};
use crate::market::{GamePhase, Market, StonkMarket, HISTORICAL_SIZE, MAX_EVENTS_PER_NIGHT};
use crate::ssh_backend::SSHBackend;
use crate::tui::Tui;
use crate::ui::UiOptions;
use crate::utils::{load_agents, load_market, save_agents, save_market, AppResult};
use async_trait::async_trait;
use crossterm::event::*;
use rand::seq::SliceRandom;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::Alphanumeric;
use russh::{server::*, Channel, ChannelId, CryptoVec, Disconnect, Pty};
use russh_keys::key::PublicKey;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use strum::IntoEnumIterator;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

pub type Password = u64;
pub type AgentsDatabase = HashMap<String, (SystemTime, UserAgent)>;

const SERVER_SSH_PORT: u16 = 3333;
const CLIENTS_DROPOUT_TIME_SECONDS: u64 = 60 * 10;
const PERSISTED_CLIENTS_DROPOUT_TIME_SECONDS: u64 = 60 * 60 * 24;
const MARKET_TICK_INTERVAL_MILLIS: u64 = 1000;
const RENDER_INTERVAL_MILLIS: u64 = 50;
const SAVE_TO_STORE_INTERVAL_SECONDS: u64 = 6;
const MIN_USER_LENGTH: usize = 3;
const MAX_USER_LENGTH: usize = 16;

static AUTH_PASSWORD_SALT: &'static str = "gbasfhgE4Fvb";
static AUTH_PUBLIC_KEY_SALT: &'static str = "fa2RR4fq9XX9";

pub fn save_keys(signing_key: &ed25519_dalek::SigningKey) -> AppResult<()> {
    let file = File::create::<&str>("./keys".into())?;
    assert!(file.metadata()?.is_file());
    let mut buffer = std::io::BufWriter::new(file);
    buffer.write(&signing_key.to_bytes())?;
    Ok(())
}

pub fn load_keys() -> AppResult<ed25519_dalek::SigningKey> {
    let file = File::open::<&str>("./keys".into())?;
    let mut buffer = std::io::BufReader::new(file);
    let mut buf: [u8; 32] = [0; 32];
    buffer.read(&mut buf)?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&buf))
}

#[derive(Clone)]
pub struct TerminalHandle {
    handle: Handle,
    // The sink collects the data which is finally flushed to the handle.
    sink: Vec<u8>,
    channel_id: ChannelId,
}

impl Debug for TerminalHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalHandle")
            .field("sink", &self.sink)
            .field("channel_id", &self.channel_id)
            .finish()
    }
}

impl TerminalHandle {
    pub async fn close(&self) -> AppResult<()> {
        self.handle
            .close(self.channel_id)
            .await
            .map_err(|_| anyhow::anyhow!("Close terminal error"))?;
        self.handle
            .disconnect(Disconnect::ByApplication, "Game quit".into(), "".into())
            .await?;
        Ok(())
    }

    async fn _flush(&self) -> std::io::Result<usize> {
        let handle = self.handle.clone();
        let channel_id = self.channel_id.clone();
        let data: CryptoVec = self.sink.clone().into();
        let data_length = data.len();
        if let Err(err_data) = handle.data(channel_id, data).await {
            return Ok(err_data.len());
        }
        Ok(data_length)
    }
}

// The crossterm backend writes to the terminal handle.
impl std::io::Write for TerminalHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.sink.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        futures::executor::block_on(self._flush())?;
        self.sink.clear();
        Ok(())
    }
}

struct Client {
    tui: Tui,
    ui_options: UiOptions,
    last_action: SystemTime,
    agent: UserAgent,
}

impl Client {
    pub fn handle_key_events(&mut self, key_event: KeyEvent, market: &Market) -> AppResult<()> {
        match key_event.code {
            crossterm::event::KeyCode::Enter | crossterm::event::KeyCode::Backspace => {
                match market.phase {
                    GamePhase::Day { .. } => {
                        if let Some(_) = self.ui_options.focus_on_stonk {
                            self.ui_options.reset();
                        } else {
                            self.ui_options.select_stonk();
                        }
                    }
                    GamePhase::Night { .. } => {
                        if self.agent.selected_action().is_none() {
                            if let Some(idx) = self.ui_options.selected_event_card {
                                if idx < self.agent.available_night_events().len() {
                                    let action = self.agent.available_night_events()[idx].action();
                                    self.agent.select_action(action);
                                }
                            }
                        }
                    }
                }
            }

            KeyCode::Char('b') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };

                let stonk = &market.stonks[stonk_id];
                let amount = if key_event.modifiers == KeyModifiers::SHIFT {
                    100
                } else {
                    1
                }
                .min(stonk.available_amount());

                self.agent
                    .select_action(AgentAction::Buy { stonk_id, amount })
            }

            KeyCode::Char('m') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let stonk = &market.stonks[stonk_id];
                let amount = (self.agent.cash() / stonk.buy_price()).min(stonk.available_amount());
                self.agent
                    .select_action(AgentAction::Buy { stonk_id, amount })
            }

            KeyCode::Char('s') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let amount = if key_event.modifiers == KeyModifiers::SHIFT {
                    100
                } else {
                    1
                };
                self.agent
                    .select_action(AgentAction::Sell { stonk_id, amount })
            }

            KeyCode::Char('d') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let amount = self.agent.owned_stonks()[stonk_id];
                self.agent
                    .select_action(AgentAction::Sell { stonk_id, amount })
            }

            key_code => {
                self.ui_options.handle_key_events(key_code, &self.agent)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct SessionAuth {
    username: String,
    hashed_password: u64,
}

#[derive(Clone)]
pub struct AppServer {
    market: Arc<Mutex<Market>>,
    clients: Arc<Mutex<HashMap<usize, Client>>>,
    persisted_agents: Arc<Mutex<AgentsDatabase>>,
    session_auth: Option<SessionAuth>,
    id: usize,
}

impl AppServer {
    fn check_agent_password(agent: &UserAgent, password: u64) -> bool {
        agent.password == password
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
            load_market().unwrap_or_default()
        };
        let persisted_agents = if reset {
            let agents = AgentsDatabase::default();
            save_agents(&agents)?;
            agents
        } else {
            load_agents().unwrap_or_default()
        };
        info!("Loaded {} agents from store", persisted_agents.len());

        Ok(Self {
            market: Arc::new(Mutex::new(market)),
            clients: Arc::new(Mutex::new(HashMap::new())),
            persisted_agents: Arc::new(Mutex::new(persisted_agents)),
            session_auth: None,
            id: 0,
        })
    }

    pub async fn run(&mut self) -> AppResult<()> {
        info!("Starting SSH server. Press Ctrl-C to exit.");
        let clients = self.clients.clone();
        let persisted_agents = self.persisted_agents.clone();
        let market = self.market.clone();

        tokio::spawn(async move {
            let mut last_market_tick = SystemTime::now();
            let mut last_save_to_store = SystemTime::now();
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(RENDER_INTERVAL_MILLIS))
                    .await;

                let mut clients = clients.lock().await;
                let mut persisted_agents = persisted_agents.lock().await;
                let mut market = market.lock().await;

                clients.retain(|_, c| {
                    c.last_action.elapsed().expect("Time flows")
                        <= Duration::from_secs(CLIENTS_DROPOUT_TIME_SECONDS)
                });

                // Persist to disk
                if last_save_to_store.elapsed().expect("Time flows backwards")
                    > Duration::from_secs(SAVE_TO_STORE_INTERVAL_SECONDS)
                {
                    // Drop agents and release their stocks.
                    // for (_, (t, agent)) in persisted_agents.iter() {
                    //     if t.elapsed().expect("Time flows backwards")
                    //         <= Duration::from_secs(PERSISTED_CLIENTS_DROPOUT_TIME_SECONDS)
                    //     {
                    //         for stonk in market.stonks.iter_mut() {
                    //             if stonk.release_agent_stonks(agent).is_err() {
                    //                 println!("Failed to release agent stonks");
                    //             }
                    //         }
                    //     }
                    // }
                    persisted_agents.retain(|_, (t, _)| {
                        t.elapsed().expect("Time flows")
                            <= Duration::from_secs(PERSISTED_CLIENTS_DROPOUT_TIME_SECONDS)
                    });

                    for stonk in market.stonks.iter_mut() {
                        let allocated_shares = persisted_agents
                            .iter()
                            .map(|(_, (_, agent))| agent.owned_stonks()[stonk.id])
                            .sum::<u32>();
                        stonk.allocated_shares = allocated_shares;
                    }

                    save_market(&market).expect("Failed to store agents to disk");
                    last_save_to_store = SystemTime::now();
                }

                let number_of_players = clients.len();

                for (id, client) in clients.iter_mut() {
                    match market.phase {
                        GamePhase::Day { .. } => {
                            client.ui_options.render_counter = 0;
                            client.ui_options.selected_event_card = None;
                            if let Some(_) = client.agent.selected_action() {
                                market
                                    .apply_agent_action::<UserAgent>(&mut client.agent)
                                    .unwrap_or_else(|e| {
                                        error!("Could not apply agent {} action: {}", id, e)
                                    });
                                save_agents(&persisted_agents)
                                    .unwrap_or_else(|_| error!("Could not store agents to disk"));
                            }
                        }
                        GamePhase::Night { .. } => {
                            // At the beginning of the night, set the available events.
                            // We set them here because we need the market data.
                            if client.ui_options.render_counter == 0
                                && client.agent.available_night_events().len() == 0
                            {
                                let mut events = NightEvent::iter()
                                    .filter(|e| e.condition()(&client.agent))
                                    .collect::<Vec<NightEvent>>();
                                events.shuffle(&mut rand::thread_rng());
                                events = events
                                    .iter()
                                    .take(MAX_EVENTS_PER_NIGHT)
                                    .map(|e| *e)
                                    .collect::<Vec<NightEvent>>();
                                if events.len() > 0 {
                                    client.ui_options.selected_event_card = Some(0);
                                }
                                client.agent.set_available_night_events(events);
                            }
                            client.ui_options.render_counter += 1;
                        }
                    }
                }

                for (_, client) in clients.iter_mut() {
                    client
                        .tui
                        .draw(
                            &market,
                            &client.agent,
                            &client.ui_options,
                            number_of_players,
                        )
                        .unwrap_or_else(|e| debug!("Failed to draw: {}", e));
                }

                if last_market_tick.elapsed().expect("Time flows backwards")
                    > Duration::from_millis(MARKET_TICK_INTERVAL_MILLIS)
                {
                    market.tick();
                    last_market_tick = SystemTime::now();
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
            inactivity_timeout: Some(std::time::Duration::from_secs(360)),
            auth_rejection_time: std::time::Duration::from_secs(2),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![key_pair],
            ..Default::default()
        };

        self.run_on_address(Arc::new(config), ("0.0.0.0", SERVER_SSH_PORT))
            .await?;
        Ok(())
    }
}

impl Server for AppServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let s = self.clone();
        self.id += 1;
        s
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
        {
            // Check SessionAuth validity
            let session_auth = self
                .session_auth
                .as_ref()
                .expect("Session auth should be set");

            info!("User connected with {:?}", session_auth);
            let mut persisted_agents = self.persisted_agents.lock().await;

            // If session_auth.username is in the persisted agents db, we check the password
            let agent = if let Some((_, db_agent)) = persisted_agents.get(&session_auth.username) {
                if Self::check_agent_password(db_agent, session_auth.hashed_password) == false {
                    let error_string = format!("\n\rWrong password.\n");
                    session.disconnect(Disconnect::ByApplication, error_string.as_str(), "");
                    session.close(channel.id());
                    return Ok(false);
                }
                db_agent.clone()
            }
            // Else, we check the username and persist it
            else {
                if session_auth.username.len() < MIN_USER_LENGTH
                    || session_auth.username.len() > MAX_USER_LENGTH
                {
                    let error_string = format!(
                        "\n\rInvalid username. The username must have between {} and {} characters.\n",
                        MIN_USER_LENGTH, MAX_USER_LENGTH
                    );
                    session.disconnect(Disconnect::ByApplication, error_string.as_str(), "");
                    session.close(channel.id());
                    return Ok(false);
                }
                let new_agent =
                    UserAgent::new(session_auth.username.clone(), session_auth.hashed_password);

                new_agent
            };

            let mut clients = self.clients.lock().await;

            let terminal_handle = TerminalHandle {
                handle: session.handle(),
                sink: Vec::new(),
                channel_id: channel.id(),
            };

            let backend = SSHBackend::new(terminal_handle, (160, 48));
            let mut tui = Tui::new(backend)
                .map_err(|e| anyhow::anyhow!("Failed to create terminal interface: {}", e))?;
            tui.terminal
                .clear()
                .map_err(|e| anyhow::anyhow!("Failed to clear terminal: {}", e))?;

            persisted_agents.insert(agent.username.clone(), (SystemTime::now(), agent.clone()));
            save_agents(&persisted_agents)
                .map_err(|e| anyhow::anyhow!("Failed to store agents to disk: {}", e))?;

            let client = Client {
                tui,
                ui_options: UiOptions::new(),
                last_action: SystemTime::now(),
                agent,
            };

            clients.insert(self.id, client);
        }

        Ok(true)
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let persisted_agents = self.persisted_agents.lock().await;
        let username = if let Some((_, agent)) = persisted_agents.get(user) {
            agent.username.clone()
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
        self.session_auth = Some(SessionAuth {
            username,
            hashed_password,
        });

        Ok(Auth::Accept)
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        let persisted_agents = self.persisted_agents.lock().await;
        let username = if let Some((_, agent)) = persisted_agents.get(user) {
            agent.username.clone()
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
        self.session_auth = Some(SessionAuth {
            username,
            hashed_password,
        });

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

        if let Some(client) = clients.get_mut(&self.id) {
            let event = convert_data_to_crossterm_event(data);
            debug!("{:?}", event);
            match event {
                Some(Event::Mouse(..)) => {}
                Some(Event::Key(key_event)) => match key_event.code {
                    KeyCode::Esc => {
                        client
                            .tui
                            .exit()
                            .await
                            .unwrap_or_else(|e| error!("Error exiting tui: {}", e));
                        clients.remove(&self.id);
                    }
                    _ => {
                        let market = self.market.lock().await;
                        let mut persisted_agents = self.persisted_agents.lock().await;

                        let now = SystemTime::now();
                        client.last_action = now;
                        client
                            .handle_key_events(key_event, &market)
                            .map_err(|e| anyhow::anyhow!("Error: {}", e))?;
                        let mut db_agent = client.agent.clone();
                        db_agent.clear_action();
                        persisted_agents.insert(client.agent.username.clone(), (now, db_agent));
                        client
                            .tui
                            .draw(
                                &market,
                                &client.agent,
                                &client.ui_options,
                                number_of_players,
                            )
                            .unwrap_or_else(|e| error!("Failed to draw: {}", e));
                    }
                },
                _ => {}
            }
        } else {
            session.disconnect(Disconnect::ByApplication, "Game quit", "");
            session.close(channel);
        }

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
        if let Some(client) = clients.get_mut(&self.id) {
            client
                .tui
                .resize(col_width as u16, row_height as u16)
                .map_err(|e| anyhow::anyhow!("Resize error: {}", e))?;
        }
        Ok(())
    }
}

fn convert_data_to_key_event(data: &[u8]) -> Option<KeyEvent> {
    debug!("convert_data_to_key_event: data {:?}", data);
    let (code, modifiers) = if data.len() == 1 {
        match data[0] {
            1 => (KeyCode::Home, KeyModifiers::empty()),
            2 => (KeyCode::Insert, KeyModifiers::empty()),
            3 => (KeyCode::Delete, KeyModifiers::empty()),
            4 => (KeyCode::End, KeyModifiers::empty()),
            5 => (KeyCode::PageUp, KeyModifiers::empty()),
            6 => (KeyCode::PageDown, KeyModifiers::empty()),
            13 => (KeyCode::Enter, KeyModifiers::empty()),
            // x if x >= 1 && x <= 26 => (
            //     KeyCode::Char(((x + 86) as char).to_ascii_lowercase()),
            //     KeyModifiers::CONTROL,
            // ),
            27 => (KeyCode::Esc, KeyModifiers::empty()),
            x if x >= 65 && x <= 90 => (
                KeyCode::Char((x as char).to_ascii_lowercase()),
                KeyModifiers::SHIFT,
            ),
            x if x >= 97 && x <= 122 => (KeyCode::Char(x as char), KeyModifiers::empty()),
            127 => (KeyCode::Backspace, KeyModifiers::empty()),
            _ => return None,
        }
    } else if data.len() == 3 {
        match data[2] {
            65 => (KeyCode::Up, KeyModifiers::empty()),
            66 => (KeyCode::Down, KeyModifiers::empty()),
            67 => (KeyCode::Right, KeyModifiers::empty()),
            68 => (KeyCode::Left, KeyModifiers::empty()),
            _ => return None,
        }
    } else {
        return None;
    };

    let event = KeyEvent::new(code, modifiers);
    Some(event)
}

fn decode_sgr_mouse_input(ansi_code: Vec<u8>) -> AppResult<(u8, u16, u16)> {
    // Convert u8 vector to a String
    let ansi_str = String::from_utf8(ansi_code.clone()).map_err(|_| "Invalid UTF-8 sequence")?;

    // Check the prefix
    if !ansi_str.starts_with("\x1b[<") {
        return Err("Invalid SGR ANSI mouse code".into());
    }

    let cb_mod = if ansi_str.ends_with('M') {
        0
    } else if ansi_str.ends_with('m') {
        3
    } else {
        return Err("Invalid SGR ANSI mouse code".into());
    };

    // Remove the prefix '\x1b[<' and trailing 'M'
    let code_body = &ansi_str[3..ansi_str.len() - 1];

    // Split the components
    let components: Vec<&str> = code_body.split(';').collect();

    if components.len() != 3 {
        return Err("Invalid SGR ANSI mouse code format".into());
    }

    // Parse the components
    let cb = cb_mod
        + components[0]
            .parse::<u8>()
            .map_err(|_| "Failed to parse Cb")?;
    let cx = components[1]
        .parse::<u16>()
        .map_err(|_| "Failed to parse Cx")?;
    let cy = components[2]
        .parse::<u16>()
        .map_err(|_| "Failed to parse Cy")?;

    Ok((cb, cx, cy))
}

fn convert_data_to_mouse_event(data: &[u8]) -> Option<MouseEvent> {
    let (cb, column, row) = decode_sgr_mouse_input(data.to_vec()).ok()?;
    let kind = match cb {
        0 => MouseEventKind::Down(MouseButton::Left),
        1 => MouseEventKind::Down(MouseButton::Middle),
        2 => MouseEventKind::Down(MouseButton::Right),
        3 => MouseEventKind::Up(MouseButton::Left),
        32 => MouseEventKind::Drag(MouseButton::Left),
        33 => MouseEventKind::Drag(MouseButton::Middle),
        34 => MouseEventKind::Drag(MouseButton::Right),
        35 => MouseEventKind::Moved,
        64 => MouseEventKind::ScrollUp,
        65 => MouseEventKind::ScrollDown,
        96..=255 => {
            debug!("cb {}", cb);
            return None;
        }
        _ => return None,
    };

    let event = MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::empty(),
    };

    Some(event)
}

fn convert_data_to_crossterm_event(data: &[u8]) -> Option<Event> {
    if data.starts_with(&[27, 91, 60]) {
        if let Some(event) = convert_data_to_mouse_event(data) {
            return Some(Event::Mouse(event));
        }
    } else {
        if let Some(event) = convert_data_to_key_event(data) {
            return Some(Event::Key(event));
        }
    }

    None
}
