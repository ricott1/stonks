use super::client::AppClient;
use crate::game::agent::UserAgent;
use crate::game::market::{GamePhase, Market};
use crate::ssh::TerminalEvent;
use crate::tui::Tui;
use crate::utils::{
    delete_all_data, fresh_chacha_rng, load_market, save_agent, save_market, AgentId, AppResult,
};
use itertools::Either;
use log::info;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use ratatui::crossterm::event::KeyCode;
use russh::server::{self};
use russh::server::{Config, Server};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::{self, Receiver};
use tokio::task;
use tokio::{select, time};
use tokio_util::sync::CancellationToken;

fn save_keys(signing_key: &russh::keys::PrivateKey) -> AppResult<()> {
    let file = File::create::<&str>("./keys".into())?;
    assert!(file.metadata()?.is_file());
    let mut buffer = std::io::BufWriter::new(file);
    buffer.write(&signing_key.to_bytes()?)?;
    println!("Created new keypair for SSH server.");
    Ok(())
}

fn load_keys() -> AppResult<russh::keys::PrivateKey> {
    let bytes = std::fs::read("./keys")?;
    let private_key = russh::keys::PrivateKey::from_bytes(&bytes)?;
    println!("Loaded keypair for SSH server.");
    Ok(private_key)
}

pub struct AppServer {
    next_client_id: usize,
    port: u16,
    shutdown: CancellationToken,
    client_sender: Option<Sender<(Tui, UserAgent)>>,
    terminal_event_sender: Option<Sender<(AgentId, TerminalEvent)>>,
}

impl AppServer {
    pub fn new(port: u16) -> AppResult<Self> {
        Ok(Self {
            next_client_id: 0,
            port,
            shutdown: CancellationToken::new(),
            client_sender: None,
            terminal_event_sender: None,
        })
    }

    pub async fn run(&mut self, reset: bool, seed: Option<u64>) -> AppResult<()> {
        println!(
            "Starting SSH server on port {}. Press Ctrl-C to exit.",
            self.port
        );

        let private_key = load_keys().unwrap_or_else(|_| {
            let key = russh::keys::PrivateKey::random(
                &mut fresh_chacha_rng(),
                russh::keys::Algorithm::Ed25519,
            )
            .expect("Failed to generate SSH keys.");

            save_keys(&key).expect("Failed to save SSH keys.");
            key
        });

        let config = Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
            auth_rejection_time: std::time::Duration::from_secs(3),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![private_key],
            ..Default::default()
        };

        let shutdown = self.shutdown.clone();
        let ready_channel_shutdown = CancellationToken::new();

        let (client_sender, client_receiver) = mpsc::channel(1);
        self.client_sender = Some(client_sender);

        let (terminal_event_sender, terminal_event_receiver) = mpsc::channel(1);
        self.terminal_event_sender = Some(terminal_event_sender);

        Self::spawn_game(reset, seed, client_receiver, terminal_event_receiver);

        let server = self.run_on_address(Arc::new(config), ("0.0.0.0", self.port));

        let server_ready_channel_shutdown = ready_channel_shutdown.clone();

        let shutdown_cancelled = shutdown.cancelled();

        let result = {
            let mut server = pin!(server);
            let mut shutdown_cancelled = pin!(shutdown_cancelled);
            select! {
                result = &mut server => Either::Left(result),
                _ = &mut shutdown_cancelled => Either::Right(()),
            }
        };

        match result {
            Either::Left(result) => Ok(result?),
            Either::Right(_) => {
                println!("Shutting down");
                server_ready_channel_shutdown.cancel();
                time::sleep(Duration::from_secs(1)).await;

                Ok(())
            }
        }
    }

    fn spawn_game(
        reset: bool,
        seed: Option<u64>,
        mut client_receiver: Receiver<(Tui, UserAgent)>,
        mut terminal_event_receiver: Receiver<(AgentId, TerminalEvent)>,
    ) {
        task::spawn(async move {
            let shutdown = CancellationToken::new();
            const DRAW_TIME_STEP: Duration = Duration::from_millis(25);
            const UPDATE_TIME_STEP: Duration = Duration::from_millis(1000);

            if reset {
                info!("Resetting storage");
                delete_all_data().expect("Could not delete data");
            }

            let mut market = if let Ok(m) = load_market() {
                info!("Loading market. Starting back from {:#?}", m.phase);
                m
            } else {
                info!("Creating new market from scratch");
                let mut m = Market::default();
                let rng = &mut ChaCha8Rng::seed_from_u64(
                    seed.unwrap_or_else(|| fresh_chacha_rng().next_u64()),
                );
                m.initialize(rng);
                save_market(&m).expect("Could not save market");
                m
            };
            let mut tuis: HashMap<AgentId, Tui> = HashMap::new();
            let mut update_ticker = tokio::time::interval(UPDATE_TIME_STEP);
            let mut draw_ticker = tokio::time::interval(DRAW_TIME_STEP);

            loop {
                select! {
                    _ = update_ticker.tick() => {
                        if let Err(e) = market.tick() {
                            println!("Error ticking market: {}", e);
                            shutdown.cancel();
                        }
                    }

                    _ = draw_ticker.tick() => {
                        if matches!(market.phase, GamePhase::Night{..}) {

                            for agent_id in market.online_agents.iter() {
                                if let Some(agent) = market.agents.get_mut(agent_id) {
                                    agent.tick_render_counter();
                                }
                            }
                        }
                        let mut to_remove = vec![];
                        for tui in tuis.values_mut() {
                            tui.draw(&market, tui.id).expect("Can't draw tui");
                            if let Err(e) = tui.push_data().await {
                                println!("Error pushing to tui: {}", e);
                                let _ = tui.exit().await;
                                to_remove.push(tui.id);
                            }
                        }
                        for client_id in to_remove {
                            tuis.remove(&client_id);
                        }
                    }

                    Some((tui, agent)) = client_receiver.recv() => {
                        market.add_online_agent(tui.id, agent);
                        tuis.insert(tui.id, tui);
                    }

                    Some((client_id, event)) = terminal_event_receiver.recv() => {
                        match event {
                            TerminalEvent::Key{key_event} => {
                                match key_event.code {
                                    KeyCode::Char('q') | KeyCode::Esc => {
                                        market.remove_online_agent(client_id);
                                        if let Some(agent) = market.agents.get(&client_id) {
                                            save_agent(agent).expect("Could not save agent");
                                        }
                                        if let Some(tui) = tuis.get_mut(&client_id) {
                                            let _ = tui.exit().await;
                                        }
                                        tuis.remove(&client_id);
                                    }
                                    _ => {
                                        // Note: we don't check if the agent is online because of the uniqueness of the agent id.
                                        if let Some(agent) = market.agents.get_mut(&client_id) {
                                            agent.update_last_active_time();
                                            agent.handle_key_events(key_event, market.phase, &market.stonks);
                                            save_agent(agent).expect("Could not save agent");
                                        }
                                    }
                                }
                            }

                            _ => {}
                        }
                    }

                }

                if shutdown.is_cancelled() {
                    break;
                }
            }
            for tui in tuis.values_mut() {
                let _ = tui.exit().await;
            }

            for agent in market.agents.values() {
                save_agent(agent).expect("Could not save agent");
            }

            save_market(&market).expect("Could not save market");
        });
    }
}

impl server::Server for AppServer {
    type Handler = AppClient;
    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> AppClient {
        let client_sender = self
            .client_sender
            .as_ref()
            .expect("Tui sender should have been initialized")
            .clone();

        let terminal_event_sender = self
            .terminal_event_sender
            .as_ref()
            .expect("Tui sender should have been initialized")
            .clone();
        let client = AppClient::new(self.shutdown.clone(), client_sender, terminal_event_sender);
        self.next_client_id += 1;

        client
    }
}
