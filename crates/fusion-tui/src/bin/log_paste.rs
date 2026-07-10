use crossterm::{
    event::{self, Event, KeyCode},
    terminal,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        terminal::EnterAlternateScreen,
        event::EnableBracketedPaste
    )?;

    println!("Paste Logger. Copy your screenshot/image, press Cmd+V here, then press Esc to exit.\r");
    println!("Waiting for paste...\r");

    loop {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Paste(text) => {
                    println!("\r\n--- PASTE RECEIVED ---");
                    println!("Length: {}\r", text.len());
                    println!("Debug: {:?}\r", text);
                    println!("----------------------\r\n");
                }
                Event::Key(key) => {
                    if key.code == KeyCode::Esc {
                        break;
                    }
                    println!("Key Event: {:?}\r", key);
                }
                _ => {}
            }
        }
    }

    crossterm::execute!(
        stdout,
        event::DisableBracketedPaste,
        terminal::LeaveAlternateScreen
    )?;
    terminal::disable_raw_mode()?;
    Ok(())
}
