use crate::game::market::Market;
use crate::ui;
use crate::utils::{AgentId, AppResult};
use ratatui::crossterm;
use ratatui::crossterm::cursor::Hide;
use ratatui::crossterm::event::EnableMouseCapture;
use ratatui::crossterm::terminal::Clear;
use ratatui::crossterm::terminal::EnterAlternateScreen;
use ratatui::layout::Rect;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use ratatui::TerminalOptions;
use ratatui::Viewport;
use frittura_ssh_core::SshWriterProxy;

pub const UI_SCREEN_SIZE: (u16, u16) = (160, 50);

#[derive(Debug)]
pub struct Tui {
    pub id: AgentId,
    terminal: Terminal<CrosstermBackend<SshWriterProxy>>,
}

impl Tui {
    fn init(&mut self) -> AppResult<()> {
        crossterm::execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture,
            Clear(crossterm::terminal::ClearType::All),
            Hide
        )?;

        Ok(())
    }

    /// Restore the terminal and close the SSH channel, awaited end-to-end.
    pub async fn close(mut self) {
        self.terminal.backend_mut().writer_mut().send_and_close().await;
    }

    pub fn new(id: AgentId, writer: SshWriterProxy) -> AppResult<Self> {
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
}

