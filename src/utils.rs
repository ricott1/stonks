use crate::market::{Market, NUMBER_OF_STONKS};
use crate::ssh_server::AgentsDatabase;
use crate::stonk::Stonk;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use image::imageops::resize;
use image::io::Reader as ImageReader;
use image::{Pixel, RgbaImage};
use include_dir::{include_dir, Dir};
use ratatui::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use tracing::debug;

pub type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

static ASSETS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/");
static AGENTS_STORE_FILENAME: &'static str = "agents.json";
static MARKET_STORE_FILENAME: &'static str = "market.json";

pub fn read_image(path: &str) -> AppResult<RgbaImage> {
    let file = ASSETS_DIR.get_file(path);
    if file.is_none() {
        return Err(format!("File {} not found", path).into());
    }
    let img = ImageReader::new(Cursor::new(file.unwrap().contents()))
        .with_guessed_format()?
        .decode()?
        .into_rgba8();
    Ok(img)
}

pub fn resize_image(image: &RgbaImage, nwidth: u32, nheight: u32) -> AppResult<RgbaImage> {
    Ok(resize(
        image,
        nwidth,
        nheight,
        image::imageops::FilterType::Triangle,
    ))
}

pub fn img_to_lines<'a>(image: &RgbaImage) -> AppResult<Vec<Line<'a>>> {
    let mut lines: Vec<Line> = vec![];
    let width = image.width();
    let height = image.height();

    for y in (0..height - 1).step_by(2) {
        let mut line: Vec<Span> = vec![];

        for x in 0..width {
            let top_pixel = image.get_pixel(x, y).to_rgba();
            let btm_pixel = image.get_pixel(x, y + 1).to_rgba();
            if top_pixel[3] == 0 && btm_pixel[3] == 0 {
                line.push(Span::raw(" "));
                continue;
            }

            if top_pixel[3] > 0 && btm_pixel[3] == 0 {
                let [r, g, b, _] = top_pixel.0;
                let color = Color::Rgb(r, g, b);
                line.push(Span::styled("▀", Style::default().fg(color)));
            } else if top_pixel[3] == 0 && btm_pixel[3] > 0 {
                let [r, g, b, _] = btm_pixel.0;
                let color = Color::Rgb(r, g, b);
                line.push(Span::styled("▄", Style::default().fg(color)));
            } else {
                let [fr, fg, fb, _] = top_pixel.0;
                let fg_color = Color::Rgb(fr, fg, fb);
                let [br, bg, bb, _] = btm_pixel.0;
                let bg_color = Color::Rgb(br, bg, bb);
                line.push(Span::styled(
                    "▀",
                    Style::default().fg(fg_color).bg(bg_color),
                ));
            }
        }
        lines.push(Line::from(line));
    }
    // append last line if height is odd
    if height % 2 == 1 {
        let mut line: Vec<Span> = vec![];
        for x in 0..width {
            let top_pixel = image.get_pixel(x, height - 1).to_rgba();
            if top_pixel[3] == 0 {
                line.push(Span::raw(" "));
                continue;
            }
            let [r, g, b, _] = top_pixel.0;
            let color = Color::Rgb(r, g, b);
            line.push(Span::styled("▀", Style::default().fg(color)));
        }
        lines.push(Line::from(line));
    }

    Ok(lines)
}

fn store_path(filename: &str) -> AppResult<PathBuf> {
    let dirs = directories::ProjectDirs::from("org", "frittura", "stonks")
        .ok_or("Failed to get directories")?;
    let config_dirs = dirs.config_dir();
    if !config_dirs.exists() {
        std::fs::create_dir_all(config_dirs)?;
    }
    let path = config_dirs.join(filename);
    Ok(path)
}

fn save_to_json<T: Serialize>(path: PathBuf, data: &T) -> AppResult<()> {
    let file = File::create(path)?;
    assert!(file.metadata()?.is_file());
    let buffer = std::io::BufWriter::new(file);
    serde_json::to_writer(buffer, data)?;
    Ok(())
}

fn load_from_json<T: for<'a> Deserialize<'a>>(path: PathBuf) -> AppResult<T> {
    let file = File::open(path)?;
    let data: T = serde_json::from_reader(file)?;
    Ok(data)
}

pub fn save_agents(agents: &AgentsDatabase) -> AppResult<()> {
    save_to_json(store_path(AGENTS_STORE_FILENAME)?, agents)?;
    Ok(())
}

pub fn save_market(market: &Market) -> AppResult<()> {
    save_to_json(store_path(MARKET_STORE_FILENAME)?, market)?;
    Ok(())
}

pub fn load_agents() -> AppResult<AgentsDatabase> {
    load_from_json(store_path(AGENTS_STORE_FILENAME)?)
}

pub fn load_market() -> AppResult<Market> {
    load_from_json(store_path(MARKET_STORE_FILENAME)?)
}

pub fn load_stonks_data() -> AppResult<[Stonk; NUMBER_OF_STONKS]> {
    let file = ASSETS_DIR
        .get_file("data/stonks_data.json")
        .expect("Failed to get stonks data file");
    let data = file
        .contents_utf8()
        .expect("Failed to read stonks data file");
    let stonks = serde_json::from_str(&data)?;
    Ok(stonks)
}

pub fn save_keys(signing_key: &ed25519_dalek::SigningKey) -> AppResult<()> {
    let file = File::create::<&str>("./keys".into())?;
    assert!(file.metadata()?.is_file());
    let mut buffer = std::io::BufWriter::new(file);
    buffer.write(&signing_key.to_bytes())?;
    Ok(())
}

pub fn load_keys() -> AppResult<ed25519_dalek::SigningKey> {
    let file = File::open::<&str>("./keys".into())?;
    let mut buffer = std::io::BufReader::new(file);
    let mut buf: [u8; 32] = [0; 32];
    buffer.read(&mut buf)?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&buf))
}

fn convert_data_to_key_event(data: &[u8]) -> Option<KeyEvent> {
    debug!("convert_data_to_key_event: data {:?}", data);
    let (code, modifiers) = if data.len() == 1 {
        match data[0] {
            1 => (KeyCode::Home, KeyModifiers::empty()),
            2 => (KeyCode::Insert, KeyModifiers::empty()),
            3 => (KeyCode::Delete, KeyModifiers::empty()),
            4 => (KeyCode::End, KeyModifiers::empty()),
            5 => (KeyCode::PageUp, KeyModifiers::empty()),
            6 => (KeyCode::PageDown, KeyModifiers::empty()),
            13 => (KeyCode::Enter, KeyModifiers::empty()),
            // x if x >= 1 && x <= 26 => (
            //     KeyCode::Char(((x + 86) as char).to_ascii_lowercase()),
            //     KeyModifiers::CONTROL,
            // ),
            27 => (KeyCode::Esc, KeyModifiers::empty()),
            x if x >= 32 && x <= 64 => (KeyCode::Char(x as char), KeyModifiers::empty()),
            x if x >= 65 && x <= 90 => (
                KeyCode::Char((x as char).to_ascii_lowercase()),
                KeyModifiers::SHIFT,
            ),
            x if x >= 97 && x <= 122 => (KeyCode::Char(x as char), KeyModifiers::empty()),
            127 => (KeyCode::Backspace, KeyModifiers::empty()),
            _ => return None,
        }
    } else if data.len() == 3 {
        match data[2] {
            65 => (KeyCode::Up, KeyModifiers::empty()),
            66 => (KeyCode::Down, KeyModifiers::empty()),
            67 => (KeyCode::Right, KeyModifiers::empty()),
            68 => (KeyCode::Left, KeyModifiers::empty()),
            _ => return None,
        }
    } else {
        return None;
    };

    let event = KeyEvent::new(code, modifiers);
    Some(event)
}

fn decode_sgr_mouse_input(ansi_code: Vec<u8>) -> AppResult<(u8, u16, u16)> {
    // Convert u8 vector to a String
    let ansi_str = String::from_utf8(ansi_code.clone()).map_err(|_| "Invalid UTF-8 sequence")?;

    // Check the prefix
    if !ansi_str.starts_with("\x1b[<") {
        return Err("Invalid SGR ANSI mouse code".into());
    }

    let cb_mod = if ansi_str.ends_with('M') {
        0
    } else if ansi_str.ends_with('m') {
        3
    } else {
        return Err("Invalid SGR ANSI mouse code".into());
    };

    // Remove the prefix '\x1b[<' and trailing 'M'
    let code_body = &ansi_str[3..ansi_str.len() - 1];

    // Split the components
    let components: Vec<&str> = code_body.split(';').collect();

    if components.len() != 3 {
        return Err("Invalid SGR ANSI mouse code format".into());
    }

    // Parse the components
    let cb = cb_mod
        + components[0]
            .parse::<u8>()
            .map_err(|_| "Failed to parse Cb")?;
    let cx = components[1]
        .parse::<u16>()
        .map_err(|_| "Failed to parse Cx")?;
    let cy = components[2]
        .parse::<u16>()
        .map_err(|_| "Failed to parse Cy")?;

    Ok((cb, cx, cy))
}

fn convert_data_to_mouse_event(data: &[u8]) -> Option<MouseEvent> {
    let (cb, column, row) = decode_sgr_mouse_input(data.to_vec()).ok()?;
    let kind = match cb {
        0 => MouseEventKind::Down(MouseButton::Left),
        1 => MouseEventKind::Down(MouseButton::Middle),
        2 => MouseEventKind::Down(MouseButton::Right),
        3 => MouseEventKind::Up(MouseButton::Left),
        32 => MouseEventKind::Drag(MouseButton::Left),
        33 => MouseEventKind::Drag(MouseButton::Middle),
        34 => MouseEventKind::Drag(MouseButton::Right),
        35 => MouseEventKind::Moved,
        64 => MouseEventKind::ScrollUp,
        65 => MouseEventKind::ScrollDown,
        96..=255 => {
            debug!("cb {}", cb);
            return None;
        }
        _ => return None,
    };

    let event = MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::empty(),
    };

    Some(event)
}

pub fn convert_data_to_crossterm_event(data: &[u8]) -> Option<Event> {
    if data.starts_with(&[27, 91, 60]) {
        if let Some(event) = convert_data_to_mouse_event(data) {
            return Some(Event::Mouse(event));
        }
    } else {
        if let Some(event) = convert_data_to_key_event(data) {
            return Some(Event::Key(event));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{save_agents, AppResult};
    use crate::{
        agent::{DecisionAgent, UserAgent},
        ssh_client::SessionAuth,
    };
    use directories;
    use std::{collections::HashMap, fs::File};

    #[test]
    fn test_path() {
        let dirs = directories::ProjectDirs::from("org", "frittura", "test");
        assert!(dirs.is_some());
        let dirs_ok = dirs.unwrap();
        let config_dirs = dirs_ok.config_dir();
        println!("{:?}", config_dirs);
        if !config_dirs.exists() {
            std::fs::create_dir_all(config_dirs).unwrap();
        }
        let path = config_dirs.join("test");
        let file = File::create(path.clone());
        assert!(file.is_ok());
        assert!(path.is_file());
        if config_dirs.exists() {
            std::fs::remove_dir_all(config_dirs).unwrap();
        }
    }

    #[test]
    fn test_save() -> AppResult<()> {
        let _agents = vec![
            UserAgent::new(SessionAuth::new("username".into(), [0; 32])),
            UserAgent::new(SessionAuth::default()),
        ];

        let mut agents = HashMap::new();

        for agent in _agents.iter() {
            agents.insert(agent.username().to_string(), agent.clone());
        }

        save_agents(&agents)?;

        Ok(())
    }
}
