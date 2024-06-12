use crate::agent::UserAgent;
use crate::ssh_backend::SSHBackend;
use crate::ssh_server::TerminalHandle;
use crate::stonk::Market;
use crate::ui::{Ui, UiOptions};
use crate::utils::AppResult;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::io::{self};
use std::panic;

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
        // tui.init()?;
        Ok(tui)
    }

    /// Initializes the terminal interface.
    ///
    /// It enables the raw mode and sets terminal properties.
    pub fn init(&mut self) -> AppResult<()> {
        terminal::enable_raw_mode()?;
        crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?; //EnableMouseCapture

        // Define a custom panic hook to reset the terminal properties.
        // This way, you won't have your terminal messed up if an unexpected error happens.
        let panic_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic| {
            Self::reset().expect("failed to reset the terminal");
            panic_hook(panic);
        }));

        self.terminal.hide_cursor()?;
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
        app: &Market,
        ui_options: UiOptions,
        agent: &UserAgent,
    ) -> AppResult<()> {
        // match app.phase {
        //     GamePhase::Day { .. } => self.terminal.clear()?,
        //     GamePhase::Night { .. } => {}
        // }
        self.terminal.draw(|frame| {
            ui.render(frame, app, ui_options, agent)
                .expect("Failed rendering")
        })?;
        Ok(())
    }

    /// Resets the terminal interface.
    ///
    /// This function is also used for the panic hook to revert
    /// the terminal properties if unexpected errors occur.
    fn reset() -> AppResult<()> {
        terminal::disable_raw_mode()?;
        crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?; //DisableMouseCapture
        Ok(())
    }

    pub fn resize(&mut self, rect: Rect) -> AppResult<()> {
        self.terminal.resize(rect)?;
        self.terminal.backend_mut().size = (rect.width, rect.height);
        self.terminal.clear()?;
        Ok(())
    }

    /// Exits the terminal interface.
    ///
    /// It disables the raw mode and reverts back the terminal properties.
    pub fn exit(&mut self) -> AppResult<()> {
        self.terminal.show_cursor()?;
        self.terminal.clear()?;
        Self::reset()?;
        Ok(())
    }
}
