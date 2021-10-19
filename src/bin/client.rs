use chat::constants::{ADDR, PORT};
use chat::message::Message;
use chat::util::event::{Event, Events};
#[allow(unused_imports)]
use log::{debug, error, info, warn};
use std::io;
use std::thread;
use termion::{event::Key, input::MouseTerminal, raw::IntoRawMode, screen::AlternateScreen};
use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc;
use tui::{
    backend::TermionBackend,
    layout::{Constraint, Corner, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Terminal,
};
use unicode_width::UnicodeWidthStr;

enum InputMode {
    Normal,
    Editing,
}

struct App {
    input: String,
    input_mode: InputMode,
    messages: Vec<Message>,
    messages_max_size: usize,
}

impl Default for App {
    fn default() -> App {
        App {
            input: String::new(),
            input_mode: InputMode::Normal,
            messages: Vec::new(),
            messages_max_size: 0,
        }
    }
}

fn tui(
    client_msg_tx: mpsc::Sender<String>,
    mut server_msg_rx: mpsc::Receiver<Message>,
) -> io::Result<()> {
    // Terminal initialization
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup event handlers
    let events = Events::new();

    // Create default app state
    let mut app = App::default();

    loop {
        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(8)
                .constraints(
                    [
                        Constraint::Min(1),
                        Constraint::Length(3),
                        Constraint::Length(1),
                    ]
                    .as_ref(),
                )
                .split(f.size());

            app.messages_max_size = chunks[0].height as usize;

            let (msg, style) = match app.input_mode {
                InputMode::Normal => (
                    vec![
                        Span::raw("Press "),
                        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" to exit, "),
                        Span::styled("i", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" to start editing."),
                    ],
                    Style::default().add_modifier(Modifier::RAPID_BLINK),
                ),
                InputMode::Editing => (
                    vec![
                        Span::raw("Press "),
                        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" to stop editing, "),
                        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" to record the message"),
                    ],
                    Style::default(),
                ),
            };
            let mut text = Text::from(Spans::from(msg));
            text.patch_style(style);
            let help_message = Paragraph::new(text);
            f.render_widget(help_message, chunks[2]);

            let input = Paragraph::new(app.input.as_ref())
                .style(match app.input_mode {
                    InputMode::Normal => Style::default(),
                    InputMode::Editing => Style::default().fg(Color::Yellow),
                })
                .block(Block::default().borders(Borders::ALL).title("Input"));
            f.render_widget(input, chunks[1]);
            match app.input_mode {
                InputMode::Normal =>
                    // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
                    {}

                InputMode::Editing => {
                    // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
                    f.set_cursor(
                        // Put cursor past the end of the input text
                        chunks[1].x + app.input.width() as u16 + 1,
                        // Move one line down, from the border to the input line
                        chunks[1].y + 1,
                    )
                }
            }

            let messages: Vec<ListItem> = app
                .messages
                .iter()
                .enumerate()
                .map(|(_, m)| {
                    let content = vec![Spans::from(Span::raw(if let Some(author) = &m.author {
                        format!("{}: {}", author, m.content)
                    } else {
                        format!("{}", m.content)
                    }))];
                    ListItem::new(content)
                })
                .collect();
            let messages = List::new(messages).start_corner(Corner::BottomLeft).block(
                Block::default()
                    .border_type(BorderType::Rounded)
                    .borders(Borders::ALL)
                    .title("Messages"),
            );
            f.render_widget(messages, chunks[0]);
        })?;

        // Handle input
        if let Event::Input(input) = events.next().unwrap() {
            match app.input_mode {
                InputMode::Normal => match input {
                    Key::Char('i') => {
                        app.input_mode = InputMode::Editing;
                    }
                    Key::Char('q') => {
                        break;
                    }
                    _ => {}
                },
                InputMode::Editing => match input {
                    Key::Char('\n') => {
                        let msg: String = app.input.drain(..).collect();
                        client_msg_tx.blocking_send(msg).unwrap();
                        info!("Sent");
                    }
                    Key::Char(c) => {
                        app.input.push(c);
                    }
                    Key::Backspace => {
                        app.input.pop();
                    }
                    Key::Esc => {
                        app.input_mode = InputMode::Normal;
                    }
                    _ => {}
                },
            }
        }

        if let Ok(s) = server_msg_rx.try_recv() {
            debug!("Got msg from server = '{:?}'", s);
            app.messages.insert(0, s);
            while app.messages.len() > app.messages_max_size {
                app.messages.pop();
            }
        }
    }
    Ok(())
}

fn clear_screen() {
    for _ in 0..1000 {
        println!();
    }
}

async fn send_message(
    name: &str,
    content: String,
    stream: &mut WriteHalf<TcpStream>,
) -> io::Result<()> {
    let msg = Message {
        author: Some(name.to_owned()),
        content,
    };

    let mut buf = Vec::<u8>::new();
    msg.write_out(&mut buf);

    stream.write_all(&mut buf).await?;
    debug!("{:?} serialized is {:?}", msg, &mut buf);

    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let matches = clap::App::new("Client")
        .version("0.1")
        .author("Riley L.")
        .about("A messaging TUI.")
        .arg(
            clap::Arg::with_name("user")
                .short("u")
                .long("user")
                .value_name("DISPLAY_NAME")
                .help("Sets your display name.")
                .takes_value(true)
                .required(true),
        )
        .arg(
            clap::Arg::with_name("addr")
                .short("a")
                .long("addr")
                .value_name("SERVER_IP_ADDRESS")
                .help("Connects to the give server IP.")
                .takes_value(true)
                .required(true),
        )
        .get_matches();
    let display_name = matches.value_of("user").unwrap().to_owned();
    let matched_addr = matches.value_of("addr").unwrap_or(ADDR);
    let desired_addr = if matched_addr.split(":").collect::<Vec<_>>().len() == 1 {
        format!("{}:{}", matched_addr, PORT)
    } else {
        matched_addr.to_owned()
    };

    // clear_screen();

    let (mut recv_stream, mut send_stream) =
        tokio::io::split(TcpStream::connect(desired_addr).await?);

    let (client_msg_tx, mut client_msg_rx) = mpsc::channel(100);
    let (server_msg_tx, server_msg_rx) = mpsc::channel(100);

    let tui_thread_task = tokio::spawn(async move {
        let tui_thread = thread::spawn(move || tui(client_msg_tx, server_msg_rx));
        tui_thread.join().unwrap().unwrap();
    });

    let client_msg_task = tokio::spawn(async move {
        while let Some(new_msg) = client_msg_rx.recv().await {
            send_message(display_name.as_ref(), new_msg, &mut send_stream)
                .await
                .unwrap();
        }
        info!("Channel closed.");
    });

    let server_msg_task = tokio::spawn(async move {
        let mut buf = [0u8; 128];
        debug!("Starting up.");
        while let Ok(n) = recv_stream.read(&mut buf[..]).await {
            if n == 0 {
                info!("Connection dropped.");
                break;
            }
            let msg = Message::read_in(&buf[..]);
            debug!("Got {:?} from the server", msg);

            server_msg_tx.send(msg).await.unwrap();
        }
        debug!("Done.");
    });

    select! {
        _ = client_msg_task => {}
        _ = server_msg_task => {}
        _ = tui_thread_task => {}
    }

    clear_screen();

    Ok(())
}
