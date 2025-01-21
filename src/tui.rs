use crate::game::market::Market;
use crate::ssh::SSHWriterProxy;
use crate::ui;
use crate::utils::{AgentId, AppResult};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::Clear;
use crossterm::terminal::SetTitle;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::layout::Rect;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use ratatui::TerminalOptions;
use ratatui::Viewport;

pub const UI_SCREEN_SIZE: (u16, u16) = (160, 50);

#[derive(Debug)]
pub struct Tui {
    pub id: AgentId,
    terminal: Terminal<CrosstermBackend<SSHWriterProxy>>,
}

impl Tui {
    fn init(&mut self) -> AppResult<()> {
        crossterm::execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture,
            SetTitle("Rebels in the sky"),
            Clear(crossterm::terminal::ClearType::All),
            Hide
        )?;

        Ok(())
    }

    pub fn new(id: AgentId, writer: SSHWriterProxy) -> AppResult<Self> {
        let backend = CrosstermBackend::new(writer);
        let opts = TerminalOptions {
            viewport: Viewport::Fixed(Rect {
                x: 0,
                y: 0,
                width: UI_SCREEN_SIZE.0,
                height: UI_SCREEN_SIZE.1,
            }),
        };

        let terminal = Terminal::with_options(backend, opts)?;
        let mut tui = Self { id, terminal };

        tui.init()?;

        Ok(tui)
    }

    pub fn draw(&mut self, market: &Market, agent_id: AgentId) -> AppResult<()> {
        self.terminal.draw(|frame| {
            if let Some(agent) = market.agents.get(&agent_id) {
                let number_of_players = market.online_agents.len();
                ui::ui::render(frame, market, agent, number_of_players)
                    .expect("Error while rendering game.")
            }
        })?;
        Ok(())
    }

    pub async fn push_data(&mut self) -> AppResult<()> {
        self.terminal.backend_mut().writer_mut().send().await?;
        Ok(())
    }

    pub fn resize(&mut self, size: (u16, u16)) -> AppResult<()> {
        self.terminal.resize(Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        })?;
        Ok(())
    }

    pub async fn exit(&mut self) -> AppResult<()> {
        crossterm::execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Clear(crossterm::terminal::ClearType::All),
            Show
        )?;

        self.terminal.backend_mut().writer_mut().send().await?;

        Ok(())
    }
}
