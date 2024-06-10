use crate::ssh_backend::SSHBackend;
use crate::stonk::App;
use crate::tui::Tui;
use crate::ui::Ui;
use crate::utils::AppResult;
use async_trait::async_trait;
use russh::{server::*, Channel, ChannelId, CryptoVec, Disconnect};
use russh_keys::key::PublicKey;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

const SERVER_SSH_PORT: u16 = 3333;

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

#[derive(Clone)]
struct TerminalHandle {
    handle: Handle,
    // The sink collects the data which is finally flushed to the handle.
    sink: Vec<u8>,
    channel_id: ChannelId,
}

impl TerminalHandle {
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

#[derive(Clone)]
pub struct AppServer {
    app: Arc<Mutex<App>>,
    clients: Arc<Mutex<HashMap<usize, Tui<SSHBackend<TerminalHandle>>>>>,
    id: usize,
}

impl AppServer {
    pub fn new(app: App) -> Self {
        Self {
            app: Arc::new(Mutex::new(app)),
            clients: Arc::new(Mutex::new(HashMap::new())),
            id: 0,
        }
    }

    pub async fn run(&mut self) -> AppResult<()> {
        println!("Starting SSH server. Press Ctrl-C to exit.");
        let clients = self.clients.clone();
        let app = Arc::clone(&self.app);
        tokio::spawn(async move {
            let mut ui = Ui::new();
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

                let mut to_remove = Vec::new();

                let mut clients = clients.lock().await;
                let mut app = app.lock().await;

                app.tick();
                println!("Last tick {:6}, {:4} clients", app.last_tick, clients.len());
                println!("Stonks prices {}", app.stonks[0].historical_prices.len());

                for (id, tui) in clients.iter_mut() {
                    tui.draw(&mut ui, &app)
                        .unwrap_or_else(|_| to_remove.push(id));
                }

                // for id in to_remove {
                //     clients.remove(&id);
                // }
            }
        });

        let signing_key = load_keys().unwrap_or_else(|_| {
            let key_pair =
                russh_keys::key::KeyPair::generate_ed25519().expect("Failed to generate key pair");
            let signing_key = match key_pair {
                russh_keys::key::KeyPair::Ed25519(signing_key) => signing_key,
            };
            save_keys(&signing_key).expect("Failed to save SSH keys.");
            signing_key
        });

        let key_pair = russh_keys::key::KeyPair::Ed25519(signing_key);

        let config = Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
            auth_rejection_time: std::time::Duration::from_secs(3),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![key_pair],
            ..Default::default()
        };

        self.run_on_address(Arc::new(config), ("0.0.0.0", SERVER_SSH_PORT))
            .await?;
        Ok(())
    }
}

impl Server for AppServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let s = self.clone();
        self.id += 1;
        s
    }
}

#[async_trait]
impl Handler for AppServer {
    type Error = anyhow::Error;

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        {
            let mut clients = self.clients.lock().await;
            let terminal_handle = TerminalHandle {
                handle: session.handle(),
                sink: Vec::new(),
                channel_id: channel.id(),
            };

            // let events = EventHandler::handler(false);
            let backend = SSHBackend::new(terminal_handle, (120, 40));

            let mut tui = Tui::new(backend)
                .map_err(|e| anyhow::anyhow!("Failed to create terminal interface: {}", e))?;
            tui.terminal
                .clear()
                .map_err(|e| anyhow::anyhow!("Failed to clear terminal: {}", e))?;

            clients.insert(self.id, tui);
        }

        Ok(true)
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let key_event = convert_data_to_key_event(data);
        let mut clients = self.clients.lock().await;
        match key_event {
            x if x.code == crossterm::event::KeyCode::Esc => {
                if let Some(tui) = clients.get_mut(&self.id) {
                    tui.terminal.clear()?;
                    tui.terminal.show_cursor().unwrap_or_else(|_| {});
                    clients.remove(&self.id);
                    session.disconnect(Disconnect::ByApplication, "Game quit", "");
                    session.close(channel);
                }
            }
            _ => {
                if let Some(tui) = clients.get_mut(&self.id) {
                    tui.terminal.clear()?;
                    // self.app.handle_key_events(key_event).unwrap_or_else(|e| {
                    //     log::error!("Failed to handle key event for client {}: {}", self.id, e)
                    // });
                } else {
                    session.disconnect(Disconnect::ByApplication, "Game quit", "");
                    session.close(channel);
                }
            }
        }

        Ok(())
    }
}

fn convert_data_to_key_event(data: &[u8]) -> crossterm::event::KeyEvent {
    let key = match data {
        b"\x1b[A" => crossterm::event::KeyCode::Up,
        b"\x1b[B" => crossterm::event::KeyCode::Down,
        b"\x1b[C" => crossterm::event::KeyCode::Right,
        b"\x1b[D" => crossterm::event::KeyCode::Left,
        b"\x03" | b"\x1b" => crossterm::event::KeyCode::Esc, // Ctrl-C is also sent as Esc
        b"\x0d" => crossterm::event::KeyCode::Enter,
        b"\x7f" => crossterm::event::KeyCode::Backspace,
        b"\x1b[3~" => crossterm::event::KeyCode::Delete,
        b"\x09" => crossterm::event::KeyCode::Tab,
        _ => crossterm::event::KeyCode::Char(data[0] as char),
    };

    crossterm::event::KeyEvent::new(key, crossterm::event::KeyModifiers::empty())
}
