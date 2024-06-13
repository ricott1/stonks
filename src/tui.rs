use crate::agent::UserAgent;
use crate::ssh_backend::SSHBackend;
use crate::stonk::Market;
use crate::ui::{Ui, UiOptions};
use crate::utils::AppResult;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;

/// Representation of a terminal user interface.
///
/// It is responsible for setting up the terminal,
/// initializing the interface and handling the draw events.
#[derive(Debug)]
pub struct Tui {
    /// Interface to the Terminal.
    pub terminal: Terminal<SSHBackend>,
}

impl Tui {
    /// Constructs a new instance of [`Tui`].
    pub fn new(backend: SSHBackend) -> AppResult<Self> {
        let terminal = Terminal::new(backend)?;
        let mut tui = Self { terminal };
        tui.init()?;

        Ok(tui)
    }

    /// Initializes the terminal interface.
    ///
    /// It enables the raw mode and sets terminal properties.
    fn init(&mut self) -> AppResult<()> {
        // crossterm::execute!(
        //     self.terminal.backend_mut(),
        //     EnterAlternateScreen,
        //     EnableMouseCapture
        // )?;

        self.terminal.clear()?;
        Ok(())
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
        self.terminal.draw(|frame| {
            ui.render(frame, market, ui_options, agent, number_of_players)
                .expect("Failed rendering")
        })?;
        Ok(())
    }

    /// Resizes the terminal interface.
    pub fn resize(&mut self, width: u16, height: u16) -> AppResult<()> {
        self.terminal.backend_mut().size = (width, height);
        self.terminal.clear()?;
        Ok(())
    }

    /// Resets the terminal interface.
    ///
    /// This function is also used for the panic hook to revert
    /// the terminal properties if unexpected errors occur.
    fn reset(&mut self) -> AppResult<()> {
        self.terminal.clear()?;
        // crossterm::execute!(
        //     self.terminal.backend_mut(),
        //     LeaveAlternateScreen,
        //     DisableMouseCapture
        // )?;
        Ok(())
    }

    /// Exits the terminal interface.
    ///
    /// It disables the raw mode and reverts back the terminal properties.
    pub async fn exit(&mut self) -> AppResult<()> {
        self.reset()?;
        self.terminal.backend().close().await
    }
}
