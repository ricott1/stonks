//! This module is a slight modification of the [`CrosstermBackend`] implementation for the [`Backend`] trait. It uses
//! the [Crossterm] crate to interact with the terminal but returns a fixed size rather than calling sys.size().
//!
//! [Crossterm]: https://crates.io/crates/crossterm
use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute, queue,
    style::{
        Attribute as CAttribute, Color as CColor, Print, SetAttribute, SetBackgroundColor,
        SetForegroundColor,
    },
    terminal::Clear,
};

use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::Cell,
    layout::Size,
    prelude::Rect,
    style::{Color, Modifier},
};

use crate::{ssh_server::TerminalHandle, utils::AppResult};

/// A [`Backend`] implementation that uses [Crossterm] to render to the terminal.
///
/// The `CrosstermBackend` struct is a wrapper around a writer implementing [`Write`], which is
/// used to send commands to the terminal. It provides methods for drawing content, manipulating
/// the cursor, and clearing the terminal screen.
///
/// Most applications should not call the methods on `CrosstermBackend` directly, but will instead
/// use the [`Terminal`] struct, which provides a more ergonomic interface.
///
/// Usually applications will enable raw mode and switch to alternate screen mode after creating
/// a `CrosstermBackend`. This is done by calling [`crossterm::terminal::enable_raw_mode`] and
/// [`crossterm::terminal::EnterAlternateScreen`] (and the corresponding disable/leave functions
/// when the application exits). This is not done automatically by the backend because it is
/// possible that the application may want to use the terminal for other purposes (like showing
/// help text) before entering alternate screen mode.
///
/// # Example
///
/// ```rust,no_run
/// use std::io::{stderr, stdout};
///
/// use crossterm::{
///     terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
///     ExecutableCommand,
/// };
/// use ratatui::prelude::*;
///
/// let mut backend = CrosstermBackend::new(stdout());
/// // or
/// let backend = CrosstermBackend::new(stderr());
/// let mut terminal = Terminal::new(backend)?;
///
/// enable_raw_mode()?;
/// stdout().execute(EnterAlternateScreen)?;
///
/// terminal.clear()?;
/// terminal.draw(|frame| {
///     // -- snip --
/// })?;
///
/// stdout().execute(LeaveAlternateScreen)?;
/// disable_raw_mode()?;
///
/// # std::io::Result::Ok(())
/// ```
///
/// See the the [examples] directory for more examples. See the [`backend`] module documentation
/// for more details on raw mode and alternate screen.
///
/// [`Write`]: std::io::Write
/// [`Terminal`]: crate::terminal::Terminal
/// [`backend`]: crate::backend
/// [Crossterm]: https://crates.io/crates/crossterm
/// [examples]: https://github.com/ratatui-org/ratatui/tree/main/examples#examples
#[derive(Debug, Clone)]
pub struct SSHBackend {
    /// The writer used to send commands to the terminal.
    writer: TerminalHandle,
    pub size: (u16, u16),
}

impl SSHBackend {
    /// Creates a new `CrosstermBackend` with the given writer.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use std::io::stdout;
    /// # use ratatui::prelude::*;
    /// let backend = CrosstermBackend::new(stdout());
    /// ```
    pub fn new(writer: TerminalHandle, size: (u16, u16)) -> SSHBackend {
        SSHBackend { writer, size }
    }

    pub async fn close(&self) -> AppResult<()> {
        self.writer.close().await
    }
}

impl Write for SSHBackend {
    /// Writes a buffer of bytes to the underlying buffer.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    /// Flushes the underlying buffer.
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl Backend for SSHBackend {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut fg = Color::Reset;
        let mut bg = Color::Reset;
        #[cfg(feature = "underline-color")]
        let mut underline_color = Color::Reset;
        let mut modifier = Modifier::empty();
        let mut last_pos: Option<(u16, u16)> = None;
        for (x, y, cell) in content {
            // Move the cursor if the previous location was not (x - 1, y)
            if !matches!(last_pos, Some(p) if x == p.0 + 1 && y == p.1) {
                queue!(self.writer, MoveTo(x, y))?;
            }
            last_pos = Some((x, y));
            if cell.modifier != modifier {
                let diff = ModifierDiff {
                    from: modifier,
                    to: cell.modifier,
                };
                diff.queue(&mut self.writer)?;
                modifier = cell.modifier;
            }
            if cell.fg != fg {
                let color = c_color_from_color(cell.fg);
                queue!(self.writer, SetForegroundColor(color))?;
                fg = cell.fg;
            }
            if cell.bg != bg {
                let color = c_color_from_color(cell.bg);
                queue!(self.writer, SetBackgroundColor(color))?;
                bg = cell.bg;
            }
            #[cfg(feature = "underline-color")]
            if cell.underline_color != underline_color {
                let color = CColor::from(cell.underline_color);
                queue!(self.writer, SetUnderlineColor(color))?;
                underline_color = cell.underline_color;
            }

            queue!(self.writer, Print(cell.symbol()))?;
        }

        #[cfg(not(feature = "underline-color"))]
        return queue!(
            self.writer,
            SetForegroundColor(CColor::Reset),
            SetBackgroundColor(CColor::Reset),
            SetAttribute(CAttribute::Reset),
        );
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Hide)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Show)
    }

    fn get_cursor(&mut self) -> io::Result<(u16, u16)> {
        crossterm::cursor::position()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    fn set_cursor(&mut self, x: u16, y: u16) -> io::Result<()> {
        execute!(self.writer, MoveTo(x, y))
    }

    fn clear(&mut self) -> io::Result<()> {
        self.clear_region(ClearType::All)
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        execute!(
            self.writer,
            Clear(match clear_type {
                ClearType::All => crossterm::terminal::ClearType::All,
                ClearType::AfterCursor => crossterm::terminal::ClearType::FromCursorDown,
                ClearType::BeforeCursor => crossterm::terminal::ClearType::FromCursorUp,
                ClearType::CurrentLine => crossterm::terminal::ClearType::CurrentLine,
                ClearType::UntilNewLine => crossterm::terminal::ClearType::UntilNewLine,
            })
        )
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        for _ in 0..n {
            queue!(self.writer, Print("\n"))?;
        }
        self.writer.flush()
    }

    fn size(&self) -> io::Result<Rect> {
        Ok(Rect::new(0, 0, self.size.0, self.size.1))
    }

    fn window_size(&mut self) -> Result<WindowSize, io::Error> {
        let rect = self.size()?;
        let (width, height) = (rect.width, rect.height);
        Ok(WindowSize {
            columns_rows: Size { width, height },
            pixels: Size { width, height },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// The `ModifierDiff` struct is used to calculate the difference between two `Modifier`
/// values. This is useful when updating the terminal display, as it allows for more
/// efficient updates by only sending the necessary changes.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
struct ModifierDiff {
    pub from: Modifier,
    pub to: Modifier,
}

impl ModifierDiff {
    fn queue<W>(&self, mut w: W) -> io::Result<()>
    where
        W: io::Write,
    {
        //use crossterm::Attribute;
        let removed = self.from - self.to;
        if removed.contains(Modifier::REVERSED) {
            queue!(w, SetAttribute(CAttribute::NoReverse))?;
        }
        if removed.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(CAttribute::NormalIntensity))?;
            if self.to.contains(Modifier::DIM) {
                queue!(w, SetAttribute(CAttribute::Dim))?;
            }
        }
        if removed.contains(Modifier::ITALIC) {
            queue!(w, SetAttribute(CAttribute::NoItalic))?;
        }
        if removed.contains(Modifier::UNDERLINED) {
            queue!(w, SetAttribute(CAttribute::NoUnderline))?;
        }
        if removed.contains(Modifier::DIM) {
            queue!(w, SetAttribute(CAttribute::NormalIntensity))?;
        }
        if removed.contains(Modifier::CROSSED_OUT) {
            queue!(w, SetAttribute(CAttribute::NotCrossedOut))?;
        }
        if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
            queue!(w, SetAttribute(CAttribute::NoBlink))?;
        }

        let added = self.to - self.from;
        if added.contains(Modifier::REVERSED) {
            queue!(w, SetAttribute(CAttribute::Reverse))?;
        }
        if added.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(CAttribute::Bold))?;
        }
        if added.contains(Modifier::ITALIC) {
            queue!(w, SetAttribute(CAttribute::Italic))?;
        }
        if added.contains(Modifier::UNDERLINED) {
            queue!(w, SetAttribute(CAttribute::Underlined))?;
        }
        if added.contains(Modifier::DIM) {
            queue!(w, SetAttribute(CAttribute::Dim))?;
        }
        if added.contains(Modifier::CROSSED_OUT) {
            queue!(w, SetAttribute(CAttribute::CrossedOut))?;
        }
        if added.contains(Modifier::SLOW_BLINK) {
            queue!(w, SetAttribute(CAttribute::SlowBlink))?;
        }
        if added.contains(Modifier::RAPID_BLINK) {
            queue!(w, SetAttribute(CAttribute::RapidBlink))?;
        }

        Ok(())
    }
}

fn c_color_from_color(color: Color) -> CColor {
    match color {
        Color::Reset => CColor::Reset,
        Color::Black => CColor::Black,
        Color::Red => CColor::DarkRed,
        Color::Green => CColor::DarkGreen,
        Color::Yellow => CColor::DarkYellow,
        Color::Blue => CColor::DarkBlue,
        Color::Magenta => CColor::DarkMagenta,
        Color::Cyan => CColor::DarkCyan,
        Color::Gray => CColor::Grey,
        Color::DarkGray => CColor::DarkGrey,
        Color::LightRed => CColor::Red,
        Color::LightGreen => CColor::Green,
        Color::LightBlue => CColor::Blue,
        Color::LightYellow => CColor::Yellow,
        Color::LightMagenta => CColor::Magenta,
        Color::LightCyan => CColor::Cyan,
        Color::White => CColor::White,
        Color::Indexed(i) => CColor::AnsiValue(i),
        Color::Rgb(r, g, b) => CColor::Rgb { r, g, b },
    }
}
