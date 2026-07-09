use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// Events the TUI reacts to.
#[derive(Debug)]
pub enum AppEvent {
    /// A key press from the user.
    Key(KeyEvent),
    /// A mouse event from the user.
    Mouse(MouseEvent),
    /// Terminal was resized.
    Resize(u16, u16),
    /// Periodic tick for animations / spinners.
    Tick,
    /// Agent produced an event (thinking, tool call, final response, etc.).
    Agent(fusion_agent::agent::AgentEvent),
    /// Bracketed paste from user.
    Paste(String),
}

/// Async event handler — polls crossterm events and agent channel.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventHandler {
    /// Spawn the event loop. Returns the handler and a sender for agent events.
    pub fn new(tick_rate_ms: u64) -> (Self, mpsc::UnboundedSender<AppEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();
 
        // Crossterm event polling task
        tokio::spawn(async move {
            loop {
                if event::poll(Duration::from_millis(tick_rate_ms)).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let app_event = match evt {
                            Event::Key(key) if key.kind == KeyEventKind::Press => {
                                Some(AppEvent::Key(key))
                            }
                            Event::Mouse(mouse) => Some(AppEvent::Mouse(mouse)),
                            Event::Resize(w, h) => Some(AppEvent::Resize(w, h)),
                            Event::Paste(text) => Some(AppEvent::Paste(text)),
                            _ => None,
                        };
                        if let Some(e) = app_event {
                            if event_tx.send(e).is_err() {
                                break;
                            }
                        }
                    }
                } else {
                    // Tick
                    if event_tx.send(AppEvent::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        (Self { rx }, tx)
    }


    /// Wait for the next event.
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }
}
