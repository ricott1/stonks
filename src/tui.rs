use crate::stonk::App;
use crate::ui::Ui;
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
pub struct Tui<B>
where
    B: ratatui::backend::Backend,
{
    /// Interface to the Terminal.
    pub terminal: Terminal<B>,
    // pub events: EventHandler,
}

impl<B> Tui<B>
where
    B: ratatui::backend::Backend,
{
    /// Constructs a new instance of [`Tui`].
    pub fn new(backend: B) -> AppResult<Self> {
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
    pub fn draw(&mut self, ui: &mut Ui, app: &App) -> AppResult<()> {
        self.terminal
            .draw(|frame| ui.render(frame, app).expect("Failed rendering"))?;
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
        Ok(())
    }

    /// Exits the terminal interface.
    ///
    /// It disables the raw mode and reverts back the terminal properties.
    pub fn exit(&mut self) -> AppResult<()> {
        self.terminal.show_cursor()?;
        Self::reset()?;
        self.terminal.clear()?;
        Ok(())
    }
}
