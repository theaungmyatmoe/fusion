use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

use fusion_agent::agent::AgentEvent;

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
    Agent(AgentEvent),
    /// Bracketed paste from user.
    Paste(String),
    /// Image was saved/extracted from clipboard asynchronously.
    ImageAttached(Result<std::path::PathBuf, String>),
}

impl AppEvent {
    /// User input that must stay snappy even while the agent streams.
    pub fn is_user_input(&self) -> bool {
        matches!(
            self,
            AppEvent::Key(_) | AppEvent::Paste(_) | AppEvent::Mouse(_)
        )
    }

    /// High-frequency stream chunks that can be coalesced before a redraw.
    /// Note: Tick is NOT included — idle ticks must not force full redraws (typing lag).
    pub fn is_stream_chunk(&self) -> bool {
        matches!(
            self,
            AppEvent::Agent(AgentEvent::Thinking(_))
                | AppEvent::Agent(AgentEvent::TextDelta(_))
                | AppEvent::Agent(AgentEvent::ToolOutputDelta { .. })
        )
    }
}

/// Async event handler — splits **user input** from **agent/app** traffic so
/// keystrokes are never stuck behind a flood of thinking tokens.
pub struct EventHandler {
    input_rx: mpsc::UnboundedReceiver<AppEvent>,
    app_rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventHandler {
    /// Spawn the event loop.
    ///
    /// Returns `(handler, app_event_tx)` where `app_event_tx` is used for
    /// agent progress, image attach results, etc. Crossterm input uses a
    /// separate high-priority channel.
    pub fn new(tick_rate_ms: u64) -> (Self, mpsc::UnboundedSender<AppEvent>) {
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (app_tx, app_rx) = mpsc::unbounded_channel();

        // Crossterm event polling task → input channel only.
        // Poll interval is short so key latency stays low; Tick is emitted on a
        // slower cadence so we don't wake the UI 10×/sec while idle/typing.
        let poll_ms = 8u64; // ~125 Hz key poll — snappy on Termux + desktop
        let tick_every = Duration::from_millis(tick_rate_ms.max(50));

        tokio::spawn(async move {
            let mut last_tick = std::time::Instant::now();
            loop {
                if event::poll(Duration::from_millis(poll_ms)).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let app_event = match evt {
                            Event::Key(key) if key.kind == KeyEventKind::Press => {
                                Some(AppEvent::Key(key))
                            }
                            // Ignore key release/repeat noise (some terminals spam these)
                            Event::Key(_) => None,
                            Event::Mouse(mouse) => Some(AppEvent::Mouse(mouse)),
                            Event::Resize(w, h) => Some(AppEvent::Resize(w, h)),
                            Event::Paste(text) => Some(AppEvent::Paste(text)),
                            _ => None,
                        };
                        if let Some(e) = app_event {
                            if input_tx.send(e).is_err() {
                                break;
                            }
                        }
                    }
                } else if last_tick.elapsed() >= tick_every {
                    last_tick = std::time::Instant::now();
                    // Tick for spinner / paste-burst detection only
                    if input_tx.send(AppEvent::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        (Self { input_rx, app_rx }, app_tx)
    }

    /// Wait for the next event, **preferring user input** when both are ready.
    pub async fn next(&mut self) -> Option<AppEvent> {
        // Non-blocking prefer: if a key is already queued, take it immediately
        // so typing never waits on agent stream backlog.
        if let Ok(e) = self.input_rx.try_recv() {
            return Some(e);
        }
        if let Ok(e) = self.app_rx.try_recv() {
            return Some(e);
        }

        tokio::select! {
            biased;
            e = self.input_rx.recv() => e,
            e = self.app_rx.recv() => e,
        }
    }

    /// Non-blocking pop, preferring user input.
    pub fn try_next(&mut self) -> Option<AppEvent> {
        self.input_rx
            .try_recv()
            .ok()
            .or_else(|| self.app_rx.try_recv().ok())
    }

    /// Drain every currently queued event (input first, then app).
    pub fn drain(&mut self) -> Vec<AppEvent> {
        let mut events = Vec::new();
        while let Ok(e) = self.input_rx.try_recv() {
            events.push(e);
        }
        while let Ok(e) = self.app_rx.try_recv() {
            events.push(e);
        }
        events
    }
}

/// Merge consecutive Thinking / TextDelta / ToolOutputDelta chunks.
pub fn coalesce_events(events: Vec<AppEvent>) -> Vec<AppEvent> {
    let mut out: Vec<AppEvent> = Vec::with_capacity(events.len());
    for event in events {
        match (out.last_mut(), event) {
            (
                Some(AppEvent::Agent(AgentEvent::Thinking(buf))),
                AppEvent::Agent(AgentEvent::Thinking(chunk)),
            ) => {
                buf.push_str(&chunk);
            }
            (
                Some(AppEvent::Agent(AgentEvent::TextDelta(buf))),
                AppEvent::Agent(AgentEvent::TextDelta(chunk)),
            ) => {
                buf.push_str(&chunk);
            }
            (
                Some(AppEvent::Agent(AgentEvent::ToolOutputDelta { name: n1, output: buf })),
                AppEvent::Agent(AgentEvent::ToolOutputDelta { name: n2, output: chunk }),
            ) if *n1 == n2 => {
                buf.push_str(&chunk);
            }
            // Collapse burst of ticks into one
            (Some(AppEvent::Tick), AppEvent::Tick) => {}
            (_, event) => out.push(event),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_merges_thinking_chunks() {
        let events = vec![
            AppEvent::Agent(AgentEvent::Thinking("Hel".into())),
            AppEvent::Agent(AgentEvent::Thinking("lo".into())),
            AppEvent::Agent(AgentEvent::Thinking("!".into())),
            AppEvent::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('a'),
                crossterm::event::KeyModifiers::empty(),
            )),
            AppEvent::Agent(AgentEvent::TextDelta("world".into())),
            AppEvent::Agent(AgentEvent::TextDelta("!".into())),
        ];
        let out = coalesce_events(events);
        assert_eq!(out.len(), 3);
        match &out[0] {
            AppEvent::Agent(AgentEvent::Thinking(t)) => assert_eq!(t, "Hello!"),
            other => panic!("expected thinking, got {:?}", other),
        }
        assert!(matches!(out[1], AppEvent::Key(_)));
        match &out[2] {
            AppEvent::Agent(AgentEvent::TextDelta(t)) => assert_eq!(t, "world!"),
            other => panic!("expected text delta, got {:?}", other),
        }
    }

    #[test]
    fn coalesce_collapses_ticks() {
        let events = vec![AppEvent::Tick, AppEvent::Tick, AppEvent::Tick];
        let out = coalesce_events(events);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], AppEvent::Tick));
    }
}
