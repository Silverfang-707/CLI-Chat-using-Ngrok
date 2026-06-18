use airaa_chat::ChatMessage;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{timeout, Duration};

struct RoomState {
    tx: broadcast::Sender<String>,
    auth_hash: String,
    active_users: HashSet<String>,
}

type AppState = Arc<Mutex<HashMap<String, RoomState>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = env::args().nth(1).unwrap_or_else(|| "3000".to_string());
    let bind_addr = format!("127.0.0.1:{}", port);

    let listener = TcpListener::bind(&bind_addr).await?;
    let state: AppState = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (socket, _) = listener.accept().await?;
        let state = Arc::clone(&state);

        tokio::spawn(async move {
            let (read_half, mut writer) = socket.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            let mut current_rx: Option<broadcast::Receiver<String>> = None;

            let mut my_room: Option<String> = None;
            let mut my_name: Option<String> = None;
            let mut is_authenticated = false;

            let ghost_timeout = Duration::from_secs(45);

            loop {
                tokio::select! {
                    result = timeout(ghost_timeout, reader.read_line(&mut line)) => {
                        match result {
                            Ok(Ok(0)) => break, 
                            Ok(Ok(_)) => {
                                if let Ok(msg) = serde_json::from_str::<ChatMessage>(&line) {

                                    if msg.action == "ping" { line.clear(); continue; }

                                    if msg.action == "list_rooms" {
                                        let rooms = state.lock().await;
                                        let active_rooms: Vec<String> = rooms.keys().cloned().collect();
                                        let content = if active_rooms.is_empty() { "Grid is empty. Initialize a new room.".to_string() } else { active_rooms.join(", ") };
                                        let response = ChatMessage { action: "room_list".to_string(), room: "system".to_string(), name: "Router".to_string(), target: "".to_string(), content, auth: "".to_string() };
                                        let _ = writer.write_all((serde_json::to_string(&response).unwrap() + "\n").as_bytes()).await;
                                        line.clear(); continue;
                                    }

                                    let mut rooms = state.lock().await;
                                    let safe_room = msg.room.to_lowercase();
                                    let safe_name = msg.name.to_lowercase();

                                    if msg.action == "get_users" {
                                        if is_authenticated {
                                            if let Some(room) = rooms.get(&safe_room) {
                                                let users: Vec<String> = room.active_users.iter().cloned().collect();
                                                let response = ChatMessage { action: "user_list".to_string(), room: safe_room.clone(), name: "System".to_string(), target: "".to_string(), content: users.join(", "), auth: "".to_string() };
                                                let _ = writer.write_all((serde_json::to_string(&response).unwrap() + "\n").as_bytes()).await;
                                            }
                                        }
                                        line.clear(); continue;
                                    }

                                    if msg.action == "join_room" {
                                        if let (Some(ref old_room), Some(ref old_name)) = (my_room.clone(), my_name.clone()) {
                                            if let Some(room) = rooms.get_mut(old_room) {
                                                room.active_users.remove(old_name);
                                            }
                                        }

                                        if !rooms.contains_key(&safe_room) {
                                            let (tx, _) = broadcast::channel(100);
                                            let mut new_room = RoomState { tx: tx.clone(), auth_hash: msg.auth.clone(), active_users: HashSet::new() };
                                            new_room.active_users.insert(safe_name.clone());
                                            rooms.insert(safe_room.clone(), new_room);
                                            current_rx = Some(tx.subscribe());
                                            my_room = Some(safe_room.clone());
                                            my_name = Some(safe_name);
                                            is_authenticated = true;

                                            let mut b_msg = msg.clone(); b_msg.action = "system".to_string(); b_msg.target = "".to_string();
                                            let _ = tx.send(serde_json::to_string(&b_msg).unwrap() + "\n");
                                        } else {
                                            let room = rooms.get_mut(&safe_room).unwrap();
                                            if room.auth_hash != msg.auth {
                                                let err = ChatMessage { action: "error".to_string(), room: "system".to_string(), name: "Router".to_string(), target: "".to_string(), content: "Incorrect password.".to_string(), auth: "".to_string() };
                                                let _ = writer.write_all((serde_json::to_string(&err).unwrap() + "\n").as_bytes()).await;
                                            } else if room.active_users.contains(&safe_name) {
                                                let err = ChatMessage { action: "error".to_string(), room: "system".to_string(), name: "Router".to_string(), target: "".to_string(), content: format!("Alias '{}' is already taken.", msg.name), auth: "".to_string() };
                                                let _ = writer.write_all((serde_json::to_string(&err).unwrap() + "\n").as_bytes()).await;
                                            } else {
                                                room.active_users.insert(safe_name.clone());
                                                current_rx = Some(room.tx.subscribe());
                                                my_room = Some(safe_room.clone());
                                                my_name = Some(safe_name);
                                                is_authenticated = true;

                                                let mut b_msg = msg.clone(); b_msg.action = "system".to_string(); b_msg.target = "".to_string();
                                                let _ = room.tx.send(serde_json::to_string(&b_msg).unwrap() + "\n");
                                            }
                                        }
                                        line.clear(); continue;
                                    }

                                    if msg.action == "message" && is_authenticated {
                                        if let Some(room) = rooms.get(&safe_room) {
                                            // 🕵️ WHISPER BLACKHOLE FIX: Check if the whisper target is actually online
                                            if !msg.target.is_empty() && !room.active_users.contains(&msg.target.to_lowercase()) {
                                                let err_msg = ChatMessage { 
                                                    action: "system".to_string(), 
                                                    room: safe_room.clone(), 
                                                    name: "Router".to_string(), 
                                                    target: "".to_string(), 
                                                    content: format!("⚠️ Whisper failed: User '{}' is not in the room.", msg.target), 
                                                    auth: "".to_string() 
                                                };
                                                // Send error back only to the person who whispered, don't broadcast it
                                                let _ = writer.write_all((serde_json::to_string(&err_msg).unwrap() + "\n").as_bytes()).await;
                                            } else {
                                                let _ = room.tx.send(line.clone());
                                            }
                                        }
                                    }
                                }
                                line.clear();
                            }
                            Ok(Err(_)) => break, 
                            Err(_) => break,     
                        }
                    }

                    recv_result = async {
                        match &mut current_rx {
                            Some(rx) => rx.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        match recv_result {
                            Ok(msg) => {
                                if writer.write_all(msg.as_bytes()).await.is_err() { break; }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                let warn = ChatMessage { action: "system".to_string(), room: "system".to_string(), name: "Router".to_string(), target: "".to_string(), content: format!("⚠️ Dropped {} message(s).", n), auth: "".to_string() };
                                let _ = writer.write_all((serde_json::to_string(&warn).unwrap() + "\n").as_bytes()).await;
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }

            if let (Some(room_name), Some(user_name)) = (my_room, my_name) {
                let mut rooms = state.lock().await;
                if let Some(room) = rooms.get_mut(&room_name) {
                    room.active_users.remove(&user_name);
                    let dc_msg = ChatMessage { action: "system".to_string(), room: room_name.clone(), name: "Router".to_string(), target: "".to_string(), content: format!("--- {} disconnected ---", user_name), auth: "".to_string() };
                    let _ = room.tx.send(serde_json::to_string(&dc_msg).unwrap() + "\n");
                    if room.active_users.is_empty() { rooms.remove(&room_name); }
                }
            }
        });
    }
}