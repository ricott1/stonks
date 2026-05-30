use super::{
    events::NightEvent,
    market::{GamePhase, NUMBER_OF_STONKS},
    stonk::{Stonk, StonkClass},
};
use crate::{
    ssh::Password,
    ui::ui::{UiDisplay, UiOptions, PALETTES},
    utils::{AgentId, AppResult},
};
use anyhow::anyhow;
use ratatui::crossterm;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::SystemTime};
use strum::Display;

pub const INITIAL_USER_CASH_CENTS: u32 = 10_000 * 100;

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentAction {
    Buy { stonk_id: usize, amount: u32 },
    Sell { stonk_id: usize, amount: u32 },
    BumpStonkClass { class: StonkClass },
    CrashAll,
    OneDayUltraVision,
    CrashAgentStonks { agent_id: AgentId },
    AddCash { amount: u32 },
    AcceptBribe,
    AssassinationVictim, // This action is actually used to signal that the user got CharacterAssassinated
    GetDividends { stonk_id: usize },
}

impl AgentAction {
    pub fn stonk_id(&self) -> Option<usize> {
        match self {
            AgentAction::Buy { stonk_id, .. } => Some(*stonk_id),
            AgentAction::Sell { stonk_id, .. } => Some(*stonk_id),
            AgentAction::GetDividends { stonk_id } => Some(*stonk_id),
            _ => None,
        }
    }

    pub fn target_agent_id(&self) -> Option<AgentId> {
        match self {
            AgentAction::CrashAgentStonks { agent_id } => Some(*agent_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AgentCondition {
    Prison,
    UltraVision,
}

pub trait DecisionAgent {
    fn id(&self) -> AgentId;

    fn cash(&self) -> u32;
    fn add_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn sub_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn owned_stonks(&self) -> &[u32; NUMBER_OF_STONKS];
    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;
    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;

    fn select_action(&mut self, action: AgentAction);
    fn selected_action(&self) -> Option<&AgentAction>;
    fn clear_action(&mut self);

    fn set_available_night_events(&mut self, actions: Vec<NightEvent>);
    fn available_night_events(&self) -> &Vec<NightEvent>;

    fn insert_past_selected_actions(&mut self, action: AgentAction, tick: usize);
    fn past_selected_actions(&self) -> &HashMap<String, (usize, usize)>;

    fn apply_conditions(&mut self, current_tick: usize);
    fn add_condition(&mut self, condition: AgentCondition, until_tick: usize);
    fn has_condition(&self, condition: AgentCondition) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAgent {
    id: AgentId,
    username: String,
    password: Password,
    last_active_time: SystemTime,
    cash: u32, //in usd cents
    owned_stonks: [u32; NUMBER_OF_STONKS],
    pending_action: Option<AgentAction>,
    available_night_events: Vec<NightEvent>,
    // A map of actions selected in the past to (number of times it was selected, last tick it was selected).
    // We use the action string as key to be able to serialize, but lose the enum nested properties.
    past_selected_actions: HashMap<String, (usize, usize)>,
    conditions: Vec<(usize, AgentCondition)>,
    #[serde(skip)]
    pub ui_options: UiOptions,
}

impl UserAgent {
    pub fn new(username: String, password: Password) -> Self {
        Self {
            id: AgentId::new_v4(),
            username,
            password,
            last_active_time: SystemTime::now(),
            cash: INITIAL_USER_CASH_CENTS, // in cents
            owned_stonks: [0; NUMBER_OF_STONKS],
            pending_action: None,
            available_night_events: vec![],
            past_selected_actions: HashMap::default(),
            conditions: vec![],
            ui_options: UiOptions::default(),
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn check_password(&self, password: Password) -> bool {
        self.password == password
    }

    pub fn cash_dollars(&self) -> f64 {
        self.cash as f64 / 100.0
    }

    pub fn conditions(&self) -> &Vec<(usize, AgentCondition)> {
        &self.conditions
    }

    pub fn update_last_active_time(&mut self) {
        self.last_active_time = SystemTime::now();
    }

    pub fn render_counter(&self) -> usize {
        self.ui_options.render_counter
    }

    pub fn tick_render_counter(&mut self) {
        self.ui_options.render_counter += 1;
    }

    pub fn clear_render_counter(&mut self) {
        self.ui_options.render_counter = 0;
    }

    pub fn clear_event_card(&mut self) {
        self.ui_options.selected_event_card_index = 0;
    }

    pub fn handle_key_events(
        &mut self,
        key_event: KeyEvent,
        market_phase: GamePhase,
        stonks: &[Stonk; NUMBER_OF_STONKS],
    ) {
        if key_event.code == KeyCode::Char('?') {
            self.ui_options.show_help = !self.ui_options.show_help;
            return;
        }
        if self.ui_options.show_help {
            // Any other key dismisses the help overlay without taking effect.
            self.ui_options.show_help = false;
            return;
        }

        let num_night_events = self.available_night_events().len();

        match key_event.code {
            crossterm::event::KeyCode::Enter => match market_phase {
                GamePhase::Day { .. } => {
                    if self.ui_options.focus_on_stonk.is_some() {
                        self.ui_options.reset();
                    } else {
                        self.ui_options.select_stonk();
                    }
                }
                GamePhase::Night { .. } => {
                    if self.selected_action().is_none() {
                        let idx = self.ui_options.selected_event_card_index;
                        if idx < self.available_night_events().len() {
                            let event = &self.available_night_events()[idx];
                            let action = event.action();
                            self.select_action(action);
                        }
                    }
                }
            },

            crossterm::event::KeyCode::Backspace => match market_phase {
                GamePhase::Day { .. } => {
                    self.ui_options.reset();
                }
                GamePhase::Night { .. } => {
                    if self.selected_action().is_some() {
                        self.clear_action();
                    }
                }
            },

            KeyCode::Char('b') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };

                let stonk = &stonks[stonk_id];
                let max_buy_amount = stonk.max_buy_amount(self.cash());

                let amount = 1.min(max_buy_amount);

                println!("stonk_id: {}, amount: {}", stonk_id, amount);

                self.select_action(AgentAction::Buy { stonk_id, amount })
            }

            KeyCode::Char('B') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };

                let stonk = &stonks[stonk_id];
                let max_buy_amount = stonk.max_buy_amount(self.cash());

                let amount = 100.min(max_buy_amount);

                println!("stonk_id: {}, amount: {}", stonk_id, amount);

                self.select_action(AgentAction::Buy { stonk_id, amount })
            }

            KeyCode::Char('m') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let stonk = &stonks[stonk_id];
                let max_buy_amount = stonk.max_buy_amount(self.cash());
                self.select_action(AgentAction::Buy {
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
                let amount = 1;
                self.select_action(AgentAction::Sell { stonk_id, amount })
            }

            KeyCode::Char('S') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let amount = 100;
                self.select_action(AgentAction::Sell { stonk_id, amount })
            }

            KeyCode::Char('d') => {
                let stonk_id = if let Some(stonk_id) = self.ui_options.focus_on_stonk {
                    stonk_id
                } else {
                    self.ui_options.selected_stonk_index
                };
                let amount = self.owned_stonks()[stonk_id];
                self.select_action(AgentAction::Sell { stonk_id, amount })
            }

            crossterm::event::KeyCode::Down => {
                if let Some(index) = self.ui_options.focus_on_stonk {
                    self.ui_options.focus_on_stonk = Some((index + 1) % 8)
                } else {
                    self.ui_options.selected_stonk_index =
                        (self.ui_options.selected_stonk_index + 1) % 8;
                }
            }

            crossterm::event::KeyCode::Up => {
                if let Some(index) = self.ui_options.focus_on_stonk {
                    self.ui_options.focus_on_stonk = Some((index + 8 - 1) % 8)
                } else {
                    self.ui_options.selected_stonk_index =
                        (self.ui_options.selected_stonk_index + 8 - 1) % 8;
                }
            }

            crossterm::event::KeyCode::Left => {
                if self.selected_action().is_none() && num_night_events > 0 {
                    let idx = self.ui_options.selected_event_card_index;
                    self.ui_options.selected_event_card_index =
                        (idx + num_night_events - 1) % num_night_events;
                }
            }

            crossterm::event::KeyCode::Right => {
                if self.selected_action().is_none() && num_night_events > 0 {
                    let idx = self.ui_options.selected_event_card_index;
                    self.ui_options.selected_event_card_index = (idx + 1) % num_night_events
                }
            }

            crossterm::event::KeyCode::Char('z') => {
                self.ui_options.zoom_level = self.ui_options.zoom_level.next()
            }

            crossterm::event::KeyCode::Char('c') => {
                self.ui_options.palette_index =
                    (self.ui_options.palette_index + 1) % PALETTES.len();
            }
            crossterm::event::KeyCode::Char('p') => self.ui_options.display = UiDisplay::Portfolio,
            crossterm::event::KeyCode::Char('l') => self.ui_options.display = UiDisplay::Stonks,

            key_code => {
                for idx in 1..9 {
                    if key_code
                        == crossterm::event::KeyCode::Char(
                            format!("{idx}").chars().next().unwrap_or_default(),
                        )
                    {
                        self.ui_options.reset();
                        self.ui_options.focus_on_stonk = Some(idx - 1);
                    }
                }
            }
        }
    }
}

impl DecisionAgent for UserAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn cash(&self) -> u32 {
        self.cash
    }
    fn add_cash(&mut self, amount: u32) -> AppResult<u32> {
        self.cash += amount;
        Ok(self.cash)
    }

    fn sub_cash(&mut self, amount: u32) -> AppResult<u32> {
        if self.cash < amount {
            return Err(anyhow!("Underflow"));
        }
        self.cash -= amount;
        Ok(self.cash)
    }

    fn owned_stonks(&self) -> &[u32; NUMBER_OF_STONKS] {
        &self.owned_stonks
    }

    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]> {
        let owned = self.owned_stonks[stonk_id];
        if let Some(new_amount) = owned.checked_add(amount) {
            self.owned_stonks[stonk_id] = new_amount;
        } else {
            return Err(anyhow!("Overflow"));
        }

        Ok(&self.owned_stonks)
    }

    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]> {
        let owned = self.owned_stonks[stonk_id];
        if let Some(new_amount) = owned.checked_sub(amount) {
            self.owned_stonks[stonk_id] = new_amount;
        } else {
            return Err(anyhow!("Underflow"));
        }
        Ok(&self.owned_stonks)
    }

    fn select_action(&mut self, action: AgentAction) {
        println!("Agent selected action: {:#?}", action);
        if self.pending_action.is_none() {
            self.pending_action = Some(action);
        }
    }

    fn selected_action(&self) -> Option<&AgentAction> {
        self.pending_action.as_ref()
    }

    fn clear_action(&mut self) {
        self.pending_action = None;
    }

    fn set_available_night_events(&mut self, events: Vec<NightEvent>) {
        self.available_night_events = events;
    }

    fn available_night_events(&self) -> &Vec<NightEvent> {
        &self.available_night_events
    }

    fn insert_past_selected_actions(&mut self, action: AgentAction, tick: usize) {
        if let Some((amount, _)) = self.past_selected_actions.get(&action.to_string()) {
            self.past_selected_actions
                .insert(action.to_string(), (amount + 1, tick));
        } else {
            self.past_selected_actions
                .insert(action.to_string(), (1, tick));
        }
    }

    fn past_selected_actions(&self) -> &HashMap<String, (usize, usize)> {
        &self.past_selected_actions
    }

    fn apply_conditions(&mut self, current_tick: usize) {
        for (_, condition) in self.conditions.iter() {
            match condition {
                AgentCondition::Prison => {}
                AgentCondition::UltraVision => {}
            }
        }

        self.conditions
            .retain(|(until_tick, _)| *until_tick > current_tick);
    }

    fn add_condition(&mut self, condition: AgentCondition, until_tick: usize) {
        self.conditions.push((until_tick, condition));
    }

    fn has_condition(&self, condition: AgentCondition) -> bool {
        self.conditions
            .iter()
            .map(|(_, condition)| *condition)
            .collect::<Vec<AgentCondition>>()
            .contains(&condition)
    }
}
