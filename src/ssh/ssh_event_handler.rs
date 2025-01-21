use crate::ssh::utils::convert_data_to_crossterm_event;
use crate::utils::AgentId;
use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, MouseEvent};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::select;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug)]
pub enum TerminalEvent {
    Key { key_event: KeyEvent },
    Mouse { mouse_event: MouseEvent },
    Resize { width: u16, height: u16 },
    Quit,
}

impl Future for TerminalEvent {
    type Output = Self;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(*self)
    }
}

/// Terminal event handler.
#[derive(Debug)]
pub struct SSHEventHandler {}

impl SSHEventHandler {
    pub const CMD_RESIZE: u8 = 0x04;

    pub fn start(
        mut stdin: mpsc::Receiver<Vec<u8>>,
        terminal_event_sender: mpsc::Sender<(AgentId, TerminalEvent)>,
        client_id: AgentId,
        client_shutdown: CancellationToken,
        server_shutdown: CancellationToken,
    ) {
        {
            tokio::task::spawn(async move {
                loop {
                    select! {
                        Some(stdin) = stdin.recv() => {
                            if let Some(event) = convert_data_to_crossterm_event(&stdin) {
                                match event {
                                    CrosstermEvent::Key(key_event) => {
                                        if key_event.kind == KeyEventKind::Press {
                                            terminal_event_sender.send((client_id,TerminalEvent::Key{key_event})).await.expect("Cannot send over channel");

                                            if key_event.code == KeyCode::Esc || key_event.code == KeyCode::Char('q') {
                                                client_shutdown.cancel();
                                                break;
                                            }

                                        }
                                    }
                                    _ => {}
                                };
                            }
                        }
                        _ = client_shutdown.cancelled() => {
                                println!("Shutting down.");
                                break;
                        },

                        _ = server_shutdown.cancelled() => {
                            println!("Shutting down from server.");
                            break;
                    },


                    }
                }
            })
        };
    }
}
