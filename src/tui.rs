use crate::agent::UserAgent;
use crate::ssh_backend::SSHBackend;
use crate::ssh_server::TerminalHandle;
use crate::stonk::Market;
use crate::ui::{Ui, UiOptions};
use crate::utils::AppResult;
use ratatui::layout::Rect;
use ratatui::Terminal;

/// Representation of a terminal user interface.
///
/// It is responsible for setting up the terminal,
/// initializing the interface and handling the draw events.
#[derive(Debug)]
pub struct Tui {
    /// Interface to the Terminal.
    pub terminal: Terminal<SSHBackend<TerminalHandle>>,
}

impl Tui {
    /// Constructs a new instance of [`Tui`].
    pub fn new(backend: SSHBackend<TerminalHandle>) -> AppResult<Self> {
        let terminal = Terminal::new(backend)?;
        let tui = Self { terminal };
        Ok(tui)
    }

    /// [`Draw`] the terminal interface by [`rendering`] the widgets.
    ///
    /// [`Draw`]: ratatui::Terminal::draw
    /// [`rendering`]: crate::ui:render
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        market: &Market,
        ui_options: UiOptions,
        agent: &UserAgent,
        number_of_players: usize,
    ) -> AppResult<()> {
        // match app.phase {
        //     GamePhase::Day { .. } => self.terminal.clear()?,
        //     GamePhase::Night { .. } => {}
        // }
        self.terminal.draw(|frame| {
            ui.render(frame, market, ui_options, agent, number_of_players)
                .expect("Failed rendering")
        })?;
        Ok(())
    }

    pub fn resize(&mut self, rect: Rect) -> AppResult<()> {
        self.terminal.resize(rect)?;
        self.terminal.backend_mut().size = (rect.width, rect.height);
        self.terminal.clear()?;
        Ok(())
    }
}
