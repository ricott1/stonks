//! Centralized game-task that owns the `Market` and routes per-agent input
//! to it. Replaces the body of the old `AppServer::spawn_game`.

use crate::game::agent::UserAgent;
use crate::game::market::{GamePhase, Market};
use crate::tui::Tui;
use crate::utils::{
    delete_all_data, fresh_chacha_rng, load_market, save_agent, save_market, AgentId,
};
use log::info;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use ratatui::crossterm::event::KeyCode;
use sshhub::core::TerminalEvent;
use std::collections::HashMap;
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;

const DRAW_TIME_STEP: Duration = Duration::from_millis(25);
const UPDATE_TIME_STEP: Duration = Duration::from_millis(1000);

pub fn spawn(
    reset: bool,
    seed: Option<u64>,
    mut client_receiver: Receiver<(Tui, UserAgent)>,
    mut terminal_event_receiver: Receiver<(AgentId, TerminalEvent)>,
) {
    tokio::task::spawn(async move {
        let shutdown = CancellationToken::new();

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
                        log::error!("Error ticking market: {e}");
                        shutdown.cancel();
                    }
                }

                _ = draw_ticker.tick() => {
                    if matches!(market.phase, GamePhase::Night { .. }) {
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
                            log::warn!("Error pushing to tui: {e}");
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
                        TerminalEvent::Key(key_event) => {
                            match key_event.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    remove_agent(&mut market, &mut tuis, client_id);
                                }
                                _ => {
                                    if let Some(agent) = market.agents.get_mut(&client_id) {
                                        agent.update_last_active_time();
                                        agent.handle_key_events(key_event, market.phase, &market.stonks);
                                        save_agent(agent).expect("Could not save agent");
                                    }
                                }
                            }
                        }
                        TerminalEvent::Quit => {
                            // SSH channel closed; tear the agent down the same
                            // way an explicit Esc would.
                            remove_agent(&mut market, &mut tuis, client_id);
                        }
                        _ => {}
                    }
                }
            }

            if shutdown.is_cancelled() {
                break;
            }
        }

        for agent in market.agents.values() {
            save_agent(agent).expect("Could not save agent");
        }

        save_market(&market).expect("Could not save market");
    });
}

/// Disconnect `client_id` from the market and drop its `Tui` (whose `Drop`
/// impl sends LeaveAlternateScreen + cleanup down the SSH channel).
fn remove_agent(market: &mut Market, tuis: &mut HashMap<AgentId, Tui>, client_id: AgentId) {
    market.remove_online_agent(client_id);
    if let Some(agent) = market.agents.get(&client_id) {
        let _ = save_agent(agent);
    }
    tuis.remove(&client_id);
}
