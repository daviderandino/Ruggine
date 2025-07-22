use futures_util::{StreamExt, SinkExt};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;
use tokio::io::AsyncBufReadExt;

const API_BASE_URL: &str = "http://127.0.0.1:3000";

// --- Strutture Dati per le risposte API ---

#[derive(Deserialize, Debug)]
struct User {
    id: Uuid,
    username: String,
}

#[derive(Deserialize, Debug)]
struct Group {
    id: Uuid,
    name: String,
}

// --- Strutture Dati per i messaggi WebSocket ---

#[derive(Serialize)]
struct ClientChatMessage {
    r#type: String,
    sender_id: Uuid,
    content: String,
}

#[derive(Deserialize, Debug)]
struct ServerChatMessage {
    sender_username: String,
    content: String,
    group_name: String,
}

// --- Stato condiviso del Client ---

#[derive(Default)]
struct ClientState {
    user: Option<User>,
    group: Option<Group>,
    ws_sender: Option<tokio::sync::mpsc::Sender<WsMessage>>,
}

type SharedState = Arc<Mutex<ClientState>>;

#[tokio::main]
async fn main() {
    let state = Arc::new(Mutex::new(ClientState::default()));
    let http_client = HttpClient::new();

    println!("Benvenuto in Ruggine Client!");
    println!("Comandi disponibili:");
    println!("  /register <username>");
    println!("  /crea <group_name>");
    println!("  /invita <group_name> <username_to_invite>");
    println!("  /join <group_name>");
    println!("  /msg <message>");
    println!("  /exit");

    let mut line_reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut buffer = String::new();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();
        buffer.clear();

        if let Ok(bytes) = line_reader.read_line(&mut buffer).await {
            if bytes == 0 { // EOF (Ctrl+D)
                break;
            }

            let input = buffer.trim();
            if input.is_empty() {
                continue;
            }

            if input == "/exit" {
                break;
            }

            handle_command(input, state.clone(), http_client.clone()).await;
        }
    }
}

async fn handle_command(input: &str, state: SharedState, http_client: HttpClient) {
    let mut parts = input.split_whitespace();
    let command = parts.next().unwrap_or("");

    match command {
        "/register" => {
            if let Some(username) = parts.next() {
                register_user(username, state, http_client).await;
            } else {
                println!("Uso: /register <username>");
            }
        }
        "/crea" => {
            if let Some(group_name) = parts.next() {
                create_group(group_name, state, http_client).await;
            } else {
                println!("Uso: /crea <group_name>");
            }
        }
        "/invita" => {
            if let (Some(group_name), Some(username)) = (parts.next(), parts.next()) {
                invite_user(group_name, username, state, http_client).await;
            } else {
                println!("Uso: /invita <group_name> <username_to_invite>");
            }
        }
        "/join" => {
            if let Some(group_name) = parts.next() {
                join_group(group_name, state, http_client).await;
            } else {
                println!("Uso: /join <group_name>");
            }
        }
        "/msg" => {
            let msg_content = parts.collect::<Vec<&str>>().join(" ");
            if !msg_content.is_empty() {
                send_message(&msg_content, state).await;
            } else {
                println!("Uso: /msg <message>");
            }
        }
        _ => println!("Comando non riconosciuto: {}", command),
    }
}

async fn register_user(username: &str, state: SharedState, client: HttpClient) {
    let payload = serde_json::json!({ "username": username });
    let res = client.post(format!("{}/users/register", API_BASE_URL))
        .json(&payload)
        .send().await;

    match res {
        Ok(response) if response.status().is_success() => {
            let user = response.json::<User>().await.unwrap();
            println!("Registrazione riuscita! ID: {}", user.id);
            let mut s = state.lock().await;
            s.user = Some(user);
        }
        Ok(response) => {
            let error_text = response.text().await.unwrap_or_default();
            println!("Errore nella registrazione: {}", error_text);
        }
        Err(e) => println!("Errore di connessione: {}", e),
    }
}

async fn create_group(group_name: &str, state: SharedState, client: HttpClient) {
    let user_id = match &state.lock().await.user {
        Some(u) => u.id,
        None => {
            println!("Devi prima registrarti! Usa /register <username>");
            return;
        }
    };
    
    let payload = serde_json::json!({ "name": group_name, "creator_id": user_id });
    let res = client.post(format!("{}/groups", API_BASE_URL))
        .json(&payload)
        .send().await;
    
    match res {
        Ok(response) if response.status().is_success() => {
            let group = response.json::<Group>().await.unwrap();
            println!("Gruppo '{}' creato con successo!", group.name);
        }
        Ok(response) => {
            let error_text = response.text().await.unwrap_or_default();
            println!("Errore nella creazione del gruppo: {}", error_text);
        }
        Err(e) => println!("Errore di connessione: {}", e),
    }
}

async fn invite_user(group_name: &str, username_to_invite: &str, state: SharedState, client: HttpClient) {
    let (inviter_id, group_id) = {
        let s = state.lock().await;
        let inviter_id = match &s.user {
            Some(u) => u.id,
            None => {
                println!("Devi prima registrarti!");
                return;
            }
        };
        // Per semplicità, cerchiamo il gruppo per nome
        // In un'app reale, l'utente potrebbe essere in più gruppi
        let group_res = client.get(format!("{}/groups/by_name/{}", API_BASE_URL, group_name)).send().await;
        let group = match group_res {
            Ok(res) if res.status().is_success() => res.json::<Group>().await.unwrap(),
            _ => {
                println!("Gruppo '{}' non trovato.", group_name);
                return;
            }
        };
        (inviter_id, group.id)
    };

    let user_to_invite_res = client.get(format!("{}/users/by_username/{}", API_BASE_URL, username_to_invite)).send().await;
    let user_to_invite = match user_to_invite_res {
        Ok(res) if res.status().is_success() => res.json::<User>().await.unwrap(),
        _ => {
            println!("Utente '{}' da invitare non trovato.", username_to_invite);
            return;
        }
    };

    let payload = serde_json::json!({
        "inviter_id": inviter_id,
        "user_to_invite_id": user_to_invite.id
    });

    let res = client.post(format!("{}/groups/{}/invite", API_BASE_URL, group_id))
        .json(&payload)
        .send().await;

    match res {
        Ok(response) if response.status().is_success() => {
            println!("Invito inviato a '{}' per il gruppo '{}'.", username_to_invite, group_name);
        }
        Ok(response) => {
            let error_text = response.text().await.unwrap_or_default();
            println!("Errore nell'invio dell'invito: {}", error_text);
        }
        Err(e) => println!("Errore di connessione: {}", e),
    }
}

async fn join_group(group_name: &str, state: SharedState, client: HttpClient) {
    // Prima, implementeremo un endpoint /join lato server.
    // Per ora, questo comando si connetterà direttamente alla WebSocket.
    
    // 1. Trova l'ID del gruppo
    let group = match client.get(format!("{}/groups/by_name/{}", API_BASE_URL, group_name)).send().await {
        Ok(res) if res.status().is_success() => res.json::<Group>().await.unwrap(),
        _ => {
            println!("Gruppo '{}' non trovato.", group_name);
            return;
        }
    };
    
    // 2. Connettiti alla WebSocket
    let ws_url = format!("ws://127.0.0.1:3000/groups/{}/chat", group.id);
    let (ws_stream, _) = match connect_async(&ws_url).await {
        Ok(s) => s,
        Err(e) => {
            println!("Impossibile connettersi alla chat del gruppo: {}", e);
            return;
        }
    };

    println!("Connesso alla chat del gruppo '{}'! Ora puoi inviare messaggi con /msg.", group.name);

    let (mut write, mut read) = ws_stream.split();

    // Canale per inviare messaggi dalla console alla WebSocket
    let (tx, mut rx) = tokio::sync::mpsc::channel::<WsMessage>(32);
    
    {
        let mut s = state.lock().await;
        s.group = Some(group);
        s.ws_sender = Some(tx);
    }

    // Task per inviare messaggi
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Task per ricevere messaggi
    tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            if let WsMessage::Text(text) = msg {
                // In un client reale, qui si deserializzerebbe un JSON strutturato.
                // Per ora stampiamo il testo grezzo.
                println!("\n< {}", text);
                print!("> ");
                io::stdout().flush().unwrap();
            }
        }
        println!("\nDisconnesso dalla chat.");
    });
}

async fn send_message(content: &str, state: SharedState) {
    let s = state.lock().await;
    
    let sender_id = match &s.user {
        Some(u) => u.id,
        None => {
            println!("Non sei registrato.");
            return;
        }
    };

    if let Some(sender) = &s.ws_sender {
        let msg = ClientChatMessage {
            r#type: "message".to_string(),
            sender_id,
            content: content.to_string(),
        };
        let json_msg = serde_json::to_string(&msg).unwrap();
        if sender.send(WsMessage::Text(json_msg)).await.is_err() {
            println!("Impossibile inviare il messaggio, connessione persa.");
        }
    } else {
        println!("Non sei connesso a nessun gruppo. Usa /join <group_name>.");
    }
}