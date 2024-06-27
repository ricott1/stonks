use crate::agent::{AgentAction, DecisionAgent, UserAgent};
use crate::market::{GamePhase, Market};
use crate::ssh_backend::SSHBackend;
use crate::tui::Tui;
use crate::ui::UiOptions;
use crate::utils::*;
use crossterm::event::*;
use russh::{server::*, ChannelId, CryptoVec, Disconnect};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::time::SystemTime;

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

pub struct Client {
    pub tui: Tui,
    ui_options: UiOptions,
    username: String,
}

impl Client {
    pub fn new(username: String, handle: Handle, channel_id: ChannelId) -> AppResult<Self> {
        let terminal_handle = TerminalHandle {
            handle,
            sink: Vec::new(),
            channel_id,
        };

        let backend = SSHBackend::new(terminal_handle, (160, 48));
        let mut tui = Tui::new(backend)
            .map_err(|e| anyhow::anyhow!("Failed to create terminal interface: {}", e))?;

        tui.terminal
            .clear()
            .map_err(|e| anyhow::anyhow!("Failed to clear terminal: {}", e))?;

        Ok(Client {
            tui,
            ui_options: UiOptions::new(),
            username,
        })
    }
    pub fn draw(
        &mut self,
        market: &Market,
        agent: &UserAgent,
        number_of_players: usize,
    ) -> AppResult<()> {
        self.tui
            .draw(market, agent, &self.ui_options, number_of_players)?;
        Ok(())
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn render_counter(&self) -> usize {
        self.ui_options.render_counter
    }

    pub fn tick_render_counter(&mut self) {
        self.ui_options.render_counter += 1;
    }

    pub fn clear_ui_options(&mut self) {
        self.ui_options.render_counter = 0;
        self.ui_options.selected_event_card_index = 0;
    }

    pub fn handle_key_events(
        &mut self,
        key_event: KeyEvent,
        market: &Market,
        agent: &mut UserAgent,
    ) -> AppResult<()> {
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
                        if agent.selected_action().is_none() {
                            let idx = self.ui_options.selected_event_card_index;
                            if idx < agent.available_night_events().len() {
                                let event = agent.available_night_events()[idx].clone();
                                let action = event.action();
                                agent.select_action(action);
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
                let max_buy_amount = if stonk.buy_price() > 0 {
                    (agent.cash() / stonk.buy_price()).min(stonk.available_amount())
                } else {
                    0
                };
                let amount = if key_event.modifiers == KeyModifiers::SHIFT {
                    100
                } else {
                    1
                }
                .min(max_buy_amount);

                agent.select_action(AgentAction::Buy { stonk_id, amount })
            }

            KeyCode::Char('m') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let stonk = &market.stonks[stonk_id];
                let max_buy_amount = if stonk.buy_price() > 0 {
                    (agent.cash() / stonk.buy_price()).min(stonk.available_amount())
                } else {
                    0
                };
                agent.select_action(AgentAction::Buy {
                    stonk_id,
                    amount: max_buy_amount,
                })
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
                agent.select_action(AgentAction::Sell { stonk_id, amount })
            }

            KeyCode::Char('d') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let amount = agent.owned_stonks()[stonk_id];
                agent.select_action(AgentAction::Sell { stonk_id, amount })
            }

            key_code => {
                self.ui_options.handle_key_events(key_code, agent)?;
            }
        }
        Ok(())
    }
}

pub type Password = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionAuth {
    pub username: String,
    pub hashed_password: Password,
    pub last_active_time: SystemTime,
}

impl Default for SessionAuth {
    fn default() -> Self {
        Self {
            username: "".to_string(),
            hashed_password: [0; 32],
            last_active_time: SystemTime::now(),
        }
    }
}

impl SessionAuth {
    pub fn new(username: String, hashed_password: Password) -> Self {
        Self {
            username,
            hashed_password,
            last_active_time: SystemTime::now(),
        }
    }

    pub fn update_last_active_time(&mut self) {
        self.last_active_time = SystemTime::now();
    }

    pub fn check_password(&self, password: Password) -> bool {
        self.hashed_password == password
    }
}
