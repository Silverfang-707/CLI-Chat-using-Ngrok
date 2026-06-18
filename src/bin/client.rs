use airaa_chat::{ChatMessage, encrypt_with_key, decrypt_with_key, derive_key, derive_auth_hash, derive_whisper_key};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{prelude::*, widgets::*};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep, Duration};
use tokio_socks::tcp::Socks5Stream;

enum UiMsg {
    System(String),
    Public(String, String),
    Whisper { sender: String, recipient: String, text: String, is_outgoing: bool },
    Error(String),
}

fn get_bundled_tor() -> Option<std::path::PathBuf> {
    let mut exe_dir = env::current_exe().ok()?;
    exe_dir.pop();
    
    // Exactly as you bundled it for release
    let tor_exe = exe_dir
        .join("tor")
        .join("tor")
        .join(format!("tor{}", env::consts::EXE_SUFFIX));
        
    if tor_exe.exists() { Some(tor_exe) } else { None }
}

async fn ensure_tor_running() -> Result<Option<std::process::Child>, Box<dyn std::error::Error>> {
    if TcpStream::connect("127.0.0.1:9050").await.is_ok() {
        return Ok(None); // Tor is already running
    }

    println!("🧅 Starting bundled Tor daemon...");

    let tor_exe = match get_bundled_tor() {
        Some(path) => path,
        None => {
            println!("========================================");
            println!("TOR DAEMON NOT FOUND");
            println!("========================================");
            println!("Missing: tor/tor/tor.exe");
            return Err("Tor daemon missing from release bundle".into());
        }
    };

    let tor_dir = env::current_exe()?.parent().unwrap().join("airaa_client_tor");
    fs::create_dir_all(&tor_dir)?;
    let torrc = tor_dir.join("torrc");
    fs::write(&torrc, "SocksPort 9050\nLog notice stdout\n")?;

    let mut child = Command::new(&tor_exe)
        .current_dir(tor_exe.parent().unwrap())
        .args(["-f", torrc.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Dynamic SOCKS proxy polling
    for _ in 0..60 {
        if TcpStream::connect("127.0.0.1:9050").await.is_ok() {
            println!("✅ Tor SOCKS proxy ready.");
            return Ok(Some(child));
        }
        sleep(Duration::from_secs(1)).await;
    }

    let _ = child.kill(); // Cleanup if it hangs
    Err("Tor failed to start".into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();

    // 1. CAPTURE ARGS: Arg 1 is address, Arg 2 is Uplink URL
    let auto_addr = env::args().nth(1);
    let uplink_url = env::args().nth(2).unwrap_or_else(|| "Local".to_string());

    let mut server_addr = match auto_addr {
        Some(addr) => addr,
        None => {
            println!("\n=== Connect to Node ===");
            print!("Enter Node Address (e.g., xyz.onion or 127.0.0.1:3000): ");
            io::stdout().flush()?;
            let mut input = String::new();
            stdin.read_line(&mut input)?;
            if input.trim().is_empty() { "127.0.0.1:3000".to_string() } else { input.trim().to_string() }
        }
    };

    println!("Negotiating connection to {}...", server_addr);

    // ==========================================
    // THE DARKNET ROUTER (Self-Sufficient)
    // ==========================================
    let mut tor_child = None;

    let (read_half, mut writer) = if server_addr.ends_with(".onion") {
        println!("🧅 Darknet link detected. Locating Tor daemon...");
        
        // FIX: The host forwards port 80 to its local server. We must dial 80.
        if !server_addr.contains(':') { server_addr = format!("{}:80", server_addr); }

        match ensure_tor_running().await {
            Ok(child_opt) => tor_child = child_opt,
            Err(e) => {
                println!("❌ {} - Check if Tor is blocked by your firewall.", e);
                std::thread::sleep(std::time::Duration::from_secs(5));
                return Ok(());
            }
        }

        let stream = match Socks5Stream::connect("127.0.0.1:9050", server_addr.as_str()).await {
            Ok(s) => s.into_inner(),
            Err(_) => { 
                println!("❌ Connection to Hidden Service failed. The host might be offline."); 
                std::thread::sleep(std::time::Duration::from_secs(4));
                if let Some(mut child) = tor_child { let _ = child.kill(); }
                return Ok(()); 
            }
        };
        stream.into_split()
    } else {
        let stream = match TcpStream::connect(&server_addr).await {
            Ok(s) => s,
            Err(_) => { 
                println!("❌ Connection failed."); 
                std::thread::sleep(std::time::Duration::from_secs(3));
                return Ok(()); 
            }
        };
        stream.into_split()
    };

    let mut reader = BufReader::new(read_half);
    
    // ==========================================
    // HANDSHAKE
    // ==========================================
    let list_req = ChatMessage { action: "list_rooms".to_string(), room: String::new(), name: String::new(), target: String::new(), content: String::new(), auth: String::new() };
    writer.write_all((serde_json::to_string(&list_req)? + "\n").as_bytes()).await?;

    let mut active_rooms: Vec<String> = Vec::new();
    let mut handshake_line = String::new();
    reader.read_line(&mut handshake_line).await?;

    if let Ok(msg) = serde_json::from_str::<ChatMessage>(&handshake_line) {
        if msg.action == "room_list" {
            println!("🌐 Active Rooms on the Grid:");
            if msg.content.contains("Grid is empty") { println!("  > {}", msg.content); } 
            else { for r in msg.content.split(", ") { println!("  > {}", r); active_rooms.push(r.to_string()); } }
        }
    }

    print!("\nEnter your alias: ");
    io::stdout().flush()?;
    let mut name = String::new();
    stdin.read_line(&mut name)?;
    let name = name.trim().to_string();

    let room_name = loop {
        println!("\n1) Join Room\n2) Create Room\n> Select option: ");
        io::stdout().flush()?;
        let mut choice = String::new();
        stdin.read_line(&mut choice)?;
        match choice.trim() {
            "1" => {
                print!("Enter room to join: "); io::stdout().flush()?;
                let mut input_room = String::new(); stdin.read_line(&mut input_room)?;
                let trimmed = input_room.trim().to_lowercase();
                if active_rooms.contains(&trimmed) { break trimmed; } else { println!("❌ Room not found."); }
            }
            "2" => {
                print!("Enter new room name: "); io::stdout().flush()?;
                let mut input_room = String::new(); stdin.read_line(&mut input_room)?;
                let trimmed = input_room.trim().to_lowercase();
                if active_rooms.contains(&trimmed) { println!("⚠️ Room exists! Use option 1 to join it."); } else if trimmed.is_empty() { println!("❌ Empty."); } else { break trimmed; }
            }
            _ => println!("❌ Invalid choice."),
        }
    };

    print!("Enter Encryption Password for [{}]: ", room_name);
    io::stdout().flush()?;
    let mut password = String::new();
    stdin.read_line(&mut password)?;
    let password = password.trim().to_string();

    let aes_key = derive_key(&password, &room_name);
    let my_whisper_key = derive_whisper_key(&password, &room_name, &name);
    let auth_hash = derive_auth_hash(&password, &room_name);

    let join_text = format!("--- {} has joined the grid ---", name);
    let encrypted_join = encrypt_with_key(&aes_key, &join_text).unwrap();
    let join_msg = ChatMessage { action: "join_room".to_string(), room: room_name.clone(), name: name.clone(), target: "".to_string(), content: encrypted_join, auth: auth_hash };
    writer.write_all((serde_json::to_string(&join_msg)? + "\n").as_bytes()).await?;

    let (tx_ui, mut rx_ui) = mpsc::unbounded_channel::<UiMsg>();
    let (tx_net, mut rx_net) = mpsc::unbounded_channel::<String>();

    let name_clone = name.clone();
    let key_clone = aes_key;
    let room_clone = room_name.clone();

    tokio::spawn(async move {
        let mut line = String::new();
        let mut ping_ticker = interval(Duration::from_secs(15));

        loop {
            tokio::select! {
                _ = ping_ticker.tick() => {
                    let ping = ChatMessage { action: "ping".to_string(), room: String::new(), name: name_clone.clone(), target: String::new(), content: String::new(), auth: String::new() };
                    let _ = writer.write_all((serde_json::to_string(&ping).unwrap() + "\n").as_bytes()).await;
                }

                Some(out_text) = rx_net.recv() => {
                    if out_text == "/u" {
                        let req = ChatMessage { action: "get_users".to_string(), room: room_clone.clone(), name: name_clone.clone(), target: String::new(), content: String::new(), auth: String::new() };
                        let _ = writer.write_all((serde_json::to_string(&req).unwrap() + "\n").as_bytes()).await;
                    } else if out_text.starts_with("/w ") {
                        let parts: Vec<&str> = out_text.splitn(3, ' ').collect();
                        if parts.len() == 3 {
                            let target_name = parts[1];
                            let whisper_text = parts[2];
                            let target_key = derive_whisper_key(&password, &room_clone, target_name);
                            if let Ok(enc) = encrypt_with_key(&target_key, whisper_text) {
                                let msg = ChatMessage { action: "message".to_string(), room: room_clone.clone(), name: name_clone.clone(), target: target_name.to_string(), content: enc, auth: String::new() };
                                let _ = writer.write_all((serde_json::to_string(&msg).unwrap() + "\n").as_bytes()).await;
                                let _ = tx_ui.send(UiMsg::Whisper { sender: name_clone.clone(), recipient: target_name.to_string(), text: whisper_text.to_string(), is_outgoing: true });
                            }
                        }
                    } else {
                        if let Ok(enc) = encrypt_with_key(&key_clone, &out_text) {
                            let msg = ChatMessage { action: "message".to_string(), room: room_clone.clone(), name: name_clone.clone(), target: "".to_string(), content: enc, auth: String::new() };
                            let _ = writer.write_all((serde_json::to_string(&msg).unwrap() + "\n").as_bytes()).await;
                        }
                    }
                }

                result = tokio::time::timeout(Duration::from_secs(45), reader.read_line(&mut line)) => {
                    match result {
                        Err(_) => {
                            let _ = tx_ui.send(UiMsg::Error("Connection lost (Timeout). Press Esc to quit.".to_string()));
                            break;
                        }
                        Ok(Ok(0)) | Ok(Err(_)) => break, 
                        Ok(Ok(_)) => {}
                    }
                    if let Ok(msg) = serde_json::from_str::<ChatMessage>(&line) {
                        if msg.action == "error" {
                            let _ = tx_ui.send(UiMsg::Error(format!("{} Press Esc to quit.", msg.content)));
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            break; 
                        } else if msg.action == "user_list" {
                            let _ = tx_ui.send(UiMsg::System(format!("Active Users: {}", msg.content)));
                        } else if msg.action == "system" && msg.content.contains("disconnected") {
                            let _ = tx_ui.send(UiMsg::System(msg.content));
                        } else {
                            let decrypted = if !msg.target.is_empty() {
                                if msg.target.to_lowercase() == name_clone.to_lowercase() {
                                    decrypt_with_key(&my_whisper_key, &msg.content).ok()
                                } else { None } 
                            } else {
                                decrypt_with_key(&key_clone, &msg.content).ok()
                            };

                            if let Some(plaintext) = decrypted {
                                if msg.action == "system" {
                                    let _ = tx_ui.send(UiMsg::System(plaintext));
                                } else if !msg.target.is_empty() {
                                    let _ = tx_ui.send(UiMsg::Whisper { sender: msg.name.clone(), recipient: name_clone.clone(), text: plaintext, is_outgoing: false });
                                } else {
                                    let _ = tx_ui.send(UiMsg::Public(msg.name.clone(), plaintext));
                                }
                            }
                        }
                    }
                    line.clear();
                }
            }
        }
    });

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut messages: Vec<UiMsg> = Vec::new();
    let mut input_buffer = String::new();

    loop {
        while let Ok(msg) = rx_ui.try_recv() { messages.push(msg); }
        if messages.len() > 1000 { messages.drain(0..100); }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(4), Constraint::Min(1), Constraint::Length(4)].as_ref())
                .split(f.size());

            let header_text = format!(
                " Airaa Grid | Uplink: {} | Room: {} | Cmds: /h Help | /u Users | /w Whisper | /!exit! Quit | ESC Exit ",
                uplink_url,
                room_name
            );
            let header = Paragraph::new(header_text.cyan().bold()).block(Block::default().borders(Borders::ALL).title(" STATUS ").title_alignment(Alignment::Center));
            f.render_widget(header, chunks[0]);

            let history_height = chunks[1].height.saturating_sub(2) as usize; 
            let start_idx = messages.len().saturating_sub(history_height);
            let display_messages = &messages[start_idx..];

            let mut list_items = Vec::with_capacity(history_height);
            for m in display_messages {
                let line = match m {
                    UiMsg::System(text) => Line::from(vec![Span::styled(format!("⚡ {}", text), Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC))]),
                    UiMsg::Public(sender, text) => Line::from(vec![Span::styled(format!("{}: ", sender), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)), Span::raw(text.clone())]),
                    UiMsg::Whisper { sender, recipient, text, is_outgoing } => {
                        if *is_outgoing {
                            Line::from(vec![Span::styled(format!("🥷 [You → {}]: ", recipient), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)), Span::styled(text.clone(), Style::default().fg(Color::Magenta))])
                        } else {
                            Line::from(vec![Span::styled(format!("🥷 [Whisper from {}]: ", sender), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)), Span::styled(text.clone(), Style::default().fg(Color::Magenta))])
                        }
                    }
                    UiMsg::Error(err) => Line::from(vec![Span::styled(format!("❌ {}", err), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))]),
                };
                list_items.push(ListItem::new(line));
            }

            let messages_block = List::new(list_items).block(Block::default().borders(Borders::ALL));
            f.render_widget(messages_block, chunks[1]);

            let input_with_cursor = format!("> {}█", input_buffer);
            let input_widget = Paragraph::new(input_with_cursor).block(Block::default().borders(Borders::ALL).title(format!(" {} ", name)).title_style(Style::default().fg(Color::Green)));
            f.render_widget(input_widget, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Enter => {
                        let trimmed = input_buffer.trim().to_string();
                        if trimmed == "/!exit!" { break; }
                        if trimmed == "/h" { messages.push(UiMsg::System("Commands: /u (List Users) | /w [Alias] [Message] (Whisper) | /!exit! (Quit)".to_string())); } 
                        else if !trimmed.is_empty() { let _ = tx_net.send(trimmed.clone()); }
                        input_buffer.clear();
                    }
                    KeyCode::Char(c) => { input_buffer.push(c); }
                    KeyCode::Backspace => { input_buffer.pop(); }
                    KeyCode::Esc => { break; }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    // CLEANUP Tor process if we spawned it from the client
    if let Some(mut child) = tor_child {
        let _ = child.kill();
    }

    Ok(())
}