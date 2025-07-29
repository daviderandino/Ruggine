use eframe::egui;
use futures_util::{stream::StreamExt, SinkExt};
use reqwest::{header, Client as HttpClient}; // Importato header
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

const API_BASE_URL: &str = "http://127.0.0.1:3000";

// --- Strutture Dati e Messaggi ---

// Manteniamo questa struct semplice per il client. Non ci serve la password qui.
#[derive(Deserialize, Debug, Clone)]
struct User {
    id: Uuid,
    username: String,
}

#[derive(Deserialize, Debug, Clone)]
struct Group {
    id: Uuid,
    name: String,
}

#[derive(Serialize)]
struct WsClientMessage {
    content: String,
}

#[derive(Deserialize, Debug, Clone)]
struct WsServerMessage {
    sender_username: String,
    content: String,
}

// Nuova struct per la risposta al login
#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}


// Messaggi che l'interfaccia invia al task di rete
enum ToBackend {
    Register(String, String), // Aggiunta password
    Login(String, String),    // Nuovo comando per il login
    CreateGroup(String),
    JoinGroup(String),
    InviteUser(String, String), // group_name, username_to_invite
    SendMessage(String),
}

// Messaggi che il task di rete invia all'interfaccia
#[derive(Debug)]
enum FromBackend {
    LoggedIn(User, String), // Utente e token
    Registered,             // Semplice conferma, l'utente dovrà loggarsi
    GroupJoined(Group),
    NewMessage(WsServerMessage),
    Info(String),
    Error(String),
}

// Stato della UI di autenticazione
#[derive(PartialEq)] // Aggiunto come suggerito dal compilatore
enum AuthState {
    Login,
    Register,
}

// --- Stato dell'Applicazione GUI ---

struct RuggineApp {
    // Stato UI
    username_input: String,
    password_input: String, // Nuovo campo per la password
    create_group_input: String,
    join_group_input: String,
    invite_user_input: String,
    chat_message_input: String,
    error_message: Option<String>,
    info_message: Option<String>,
    auth_state: AuthState, // Per cambiare tra login e registrazione

    // Stato Applicazione
    current_user: Option<User>,
    auth_token: Option<String>, // Per memorizzare il JWT
    current_group: Option<Group>,
    messages: Vec<WsServerMessage>,

    // Comunicazione
    to_backend_tx: Sender<ToBackend>,
    from_backend_rx: Receiver<FromBackend>,
    _runtime: Runtime,
}

impl RuggineApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (to_backend_tx, mut to_backend_rx) = mpsc::channel(32);
        let (from_backend_tx, from_backend_rx) = mpsc::channel(32);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.spawn(async move {
            let mut client = HttpClient::new();
            let mut ws_sender: Option<Sender<WsMessage>> = None;
            let mut current_user: Option<User> = None;

            while let Some(action) = to_backend_rx.recv().await {
                match action {
                    ToBackend::Register(username, password) => {
                        let res = handle_register(&client, username, password).await;
                        let _ = from_backend_tx.send(res).await;
                    }
                    ToBackend::Login(username, password) => {
                        let res = handle_login(&client, username, password).await;
                        if let Ok(FromBackend::LoggedIn(ref user, ref token)) = res {
                            current_user = Some(user.clone());
                            
                            // Ricostruisci il client HTTP per includere il token di default per le chiamate future
                            let mut headers = header::HeaderMap::new();
                            headers.insert(
                                header::AUTHORIZATION,
                                header::HeaderValue::from_str(&format!("Bearer {}", token))
                                    .unwrap(),
                            );
                            client = HttpClient::builder()
                                .default_headers(headers)
                                .build()
                                .unwrap();
                        }
                        let _ = from_backend_tx.send(res.unwrap()).await;
                    }
                    ToBackend::CreateGroup(group_name) => {
                        if let Some(user) = &current_user {
                             let res = handle_create_group(&client, group_name, user.id).await;
                            let _ = from_backend_tx.send(res).await;
                        }
                    }
                    ToBackend::JoinGroup(group_name) => {
                         if let Some(user) = &current_user {
                            let (ws_tx, res) =
                                handle_join_group(&client, group_name, user.id, from_backend_tx.clone())
                                    .await;
                            ws_sender = ws_tx;
                            let _ = from_backend_tx.send(res).await;
                        }
                    }
                    ToBackend::InviteUser(group_name, username_to_invite) => {
                        if let Some(user) = &current_user {
                            let res =
                                handle_invite(&client, group_name, username_to_invite, user.id)
                                    .await;
                            let _ = from_backend_tx.send(res).await;
                        }
                    }
                    ToBackend::SendMessage(content) => {
                        if let Some(sender) = &ws_sender {
                            let msg = WsClientMessage { content };
                            let json_msg = serde_json::to_string(&msg).unwrap();
                            if sender.send(WsMessage::Text(json_msg)).await.is_err() {
                                let _ = from_backend_tx
                                    .send(FromBackend::Error("Connessione persa.".into()))
                                    .await;
                            }
                        }
                    }
                }
            }
        });

        Self {
            username_input: String::new(),
            password_input: String::new(),
            create_group_input: String::new(),
            join_group_input: String::new(),
            invite_user_input: String::new(),
            chat_message_input: String::new(),
            error_message: None,
            info_message: None,
            auth_state: AuthState::Login,
            current_user: None,
            auth_token: None,
            current_group: None,
            messages: Vec::new(),
            to_backend_tx,
            from_backend_rx,
            _runtime: runtime,
        }
    }
}

// --- Logica di Disegno UI ---

impl eframe::App for RuggineApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_backend_messages();

        if self.current_user.is_some() {
            self.draw_main_view(ctx);
        } else {
            self.draw_auth_view(ctx);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

impl RuggineApp {
    fn handle_backend_messages(&mut self) {
        while let Ok(msg) = self.from_backend_rx.try_recv() {
            self.error_message = None;
            self.info_message = None;
            match msg {
                FromBackend::LoggedIn(user, token) => {
                    self.current_user = Some(user);
                    self.auth_token = Some(token);
                }
                FromBackend::Registered => {
                    self.info_message =
                        Some("Registrazione avvenuta! Ora puoi effettuare il login.".into());
                    self.auth_state = AuthState::Login; // Passa alla vista di login
                }
                FromBackend::GroupJoined(group) => {
                    self.current_group = Some(group);
                    self.messages.clear();
                }
                FromBackend::NewMessage(msg) => self.messages.push(msg),
                FromBackend::Error(err) => self.error_message = Some(err),
                FromBackend::Info(info) => self.info_message = Some(info),
            }
        }
    }

    fn draw_auth_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Benvenuto in Ruggine");
                ui.add_space(20.0);

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.auth_state, AuthState::Login, "Login");
                    ui.selectable_value(&mut self.auth_state, AuthState::Register, "Registrati");
                });

                ui.add_space(10.0);
                ui.set_max_width(250.0);

                ui.label("Username:");
                ui.text_edit_singleline(&mut self.username_input);
                
                ui.label("Password:");
                ui.add(egui::TextEdit::singleline(&mut self.password_input).password(true));


                ui.add_space(10.0);

                match self.auth_state {
                    AuthState::Login => {
                        if ui.button("Login").clicked() {
                            let _ = self.to_backend_tx.try_send(ToBackend::Login(
                                self.username_input.clone(),
                                self.password_input.clone(),
                            ));
                        }
                    }
                    AuthState::Register => {
                        if ui.button("Registrati").clicked() {
                            let _ = self.to_backend_tx.try_send(ToBackend::Register(
                                self.username_input.clone(),
                                self.password_input.clone(),
                            ));
                        }
                    }
                }
                self.draw_info_error_messages(ui);
            });
        });
    }

    fn draw_main_view(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("side_panel")
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Gruppi");

                ui.separator();
                ui.label("Crea un nuovo gruppo:");
                ui.text_edit_singleline(&mut self.create_group_input);
                if ui.button("Crea").clicked() {
                    let _ = self
                        .to_backend_tx
                        .try_send(ToBackend::CreateGroup(self.create_group_input.clone()));
                }

                ui.separator();
                ui.label("Unisciti a un gruppo:");
                ui.text_edit_singleline(&mut self.join_group_input);
                if ui.button("Join").clicked() {
                    let _ = self
                        .to_backend_tx
                        .try_send(ToBackend::JoinGroup(self.join_group_input.clone()));
                }

                if let Some(group) = &self.current_group {
                    ui.separator();
                    ui.label(format!("Invita in '{}':", group.name));
                    ui.text_edit_singleline(&mut self.invite_user_input);
                    if ui.button("Invita").clicked() {
                        let _ = self.to_backend_tx.try_send(ToBackend::InviteUser(
                            group.name.clone(),
                            self.invite_user_input.clone(),
                        ));
                    }
                }

                self.draw_info_error_messages(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(group) = &self.current_group {
                ui.heading(format!("Chat: {}", group.name));
                ui.separator();

                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for msg in &self.messages {
                            ui.label(format!("{}: {}", msg.sender_username, msg.content));
                        }
                    });

                ui.separator();
                let text_edit_response = ui
                    .text_edit_singleline(&mut self.chat_message_input)
                    .on_hover_text("Scrivi un messaggio e premi Invio");

                if text_edit_response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    if !self.chat_message_input.is_empty() {
                        let _ = self
                            .to_backend_tx
                            .try_send(ToBackend::SendMessage(self.chat_message_input.clone()));
                        self.chat_message_input.clear();
                        text_edit_response.request_focus();
                    }
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.label("Crea o unisciti a un gruppo per iniziare a chattare.");
                });
            }
        });
    }

    fn draw_info_error_messages(&self, ui: &mut egui::Ui) {
        if let Some(info) = &self.info_message {
            ui.add_space(10.0);
            ui.colored_label(egui::Color32::from_rgb(150, 255, 150), info);
        }
        if let Some(err) = &self.error_message {
            ui.add_space(10.0);
            ui.colored_label(egui::Color32::RED, err);
        }
    }
}

// --- Logica di Rete Asincrona ---
async fn handle_register(client: &HttpClient, username: String, password: String) -> FromBackend {
    if username.is_empty() || password.is_empty() {
        return FromBackend::Error("Username e password non possono essere vuoti.".into());
    }
    let payload = serde_json::json!({ "username": username, "password": password });
    match client
        .post(format!("{}/users/register", API_BASE_URL))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => FromBackend::Registered,
        Ok(res) => {
            FromBackend::Error(res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into()))
        }
        Err(_) => FromBackend::Error("Impossibile connettersi al server.".into()),
    }
}

async fn handle_login(
    client: &HttpClient,
    username: String,
    password: String,
) -> Result<FromBackend, FromBackend> {
    if username.is_empty() || password.is_empty() {
        return Err(FromBackend::Error(
            "Username e password non possono essere vuoti.".into(),
        ));
    }
    let payload = serde_json::json!({ "username": username.clone(), "password": password });
    let login_res = match client
        .post(format!("{}/users/login", API_BASE_URL))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => res
            .json::<LoginResponse>()
            .await
            .map_err(|_| FromBackend::Error("Errore risposta server.".into())),
        Ok(res) => {
            return Err(FromBackend::Error(
                res.text()
                    .await
                    .unwrap_or_else(|_| "Username o password non validi".into()),
            ))
        }
        Err(_) => return Err(FromBackend::Error("Impossibile connettersi al server.".into())),
    };

    let token = login_res.unwrap().token;

    // Ora recuperiamo i dati dell'utente per l'UI, usando il token appena ottenuto
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", token.clone())).unwrap(),
    );
    let authed_client = HttpClient::builder()
        .default_headers(headers)
        .build()
        .unwrap();

    let user_res = match authed_client
        .get(format!("{}/users/by_username/{}", API_BASE_URL, username))
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => res
            .json::<User>()
            .await
            .map_err(|_| FromBackend::Error("Errore recupero dati utente.".into())),
        _ => {
            return Err(FromBackend::Error(
                "Impossibile recuperare i dati utente dopo il login.".into(),
            ))
        }
    };

    Ok(FromBackend::LoggedIn(user_res.unwrap(), token))
}

async fn handle_create_group(client: &HttpClient, name: String, creator_id: Uuid) -> FromBackend {
    if name.is_empty() {
        return FromBackend::Error("Il nome del gruppo non può essere vuoto.".into());
    }
    // NOTA: In un'app più sicura, il creator_id andrebbe letto dal token JWT nel backend,
    // piuttosto che fidarsi di quello inviato dal client.
    let payload = serde_json::json!({ "name": name.clone(), "creator_id": creator_id });
    match client
        .post(format!("{}/groups", API_BASE_URL))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            FromBackend::Info(format!("Gruppo '{}' creato.", name))
        }
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_default()),
        Err(_) => FromBackend::Error("Errore di connessione.".into()),
    }
}

async fn handle_invite(
    client: &HttpClient,
    group_name: String,
    username_to_invite: String,
    inviter_id: Uuid,
) -> FromBackend {
    if username_to_invite.is_empty() {
        return FromBackend::Error("Devi specificare un utente da invitare.".into());
    }

    let group_res = client
        .get(format!("{}/groups/by_name/{}", API_BASE_URL, group_name))
        .send()
        .await;
    let group = match group_res {
        Ok(res) if res.status().is_success() => match res.json::<Group>().await {
            Ok(g) => g,
            Err(_) => return FromBackend::Error("Errore risposta server (gruppo).".into()),
        },
        _ => return FromBackend::Error(format!("Gruppo '{}' non trovato.", group_name)),
    };

    let user_res = client
        .get(format!(
            "{}/users/by_username/{}",
            API_BASE_URL, username_to_invite
        ))
        .send()
        .await;
    let user_to_invite = match user_res {
        Ok(res) if res.status().is_success() => match res.json::<User>().await {
            Ok(u) => u,
            Err(_) => return FromBackend::Error("Errore risposta server (utente).".into()),
        },
        _ => return FromBackend::Error(format!("Utente '{}' non trovato.", username_to_invite)),
    };

    let payload =
        serde_json::json!({ "inviter_id": inviter_id, "user_to_invite_id": user_to_invite.id });
    match client
        .post(format!("{}/groups/{}/invite", API_BASE_URL, group.id))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            FromBackend::Info(format!("Invito inviato a {}.", username_to_invite))
        }
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_default()),
        Err(_) => FromBackend::Error("Errore di connessione.".into()),
    }
}

async fn handle_join_group(
    client: &HttpClient,
    group_name: String,
    user_id: Uuid,
    from_backend_tx: Sender<FromBackend>,
) -> (Option<Sender<WsMessage>>, FromBackend) {
    let group = match client
        .get(format!("{}/groups/by_name/{}", API_BASE_URL, group_name))
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => match res.json::<Group>().await {
            Ok(g) => g,
            Err(_) => return (None, FromBackend::Error("Errore risposta server.".into())),
        },
        _ => return (None, FromBackend::Error(format!("Gruppo '{}' non trovato.", group_name))),
    };

    // NOTA: Anche qui, l'autenticazione del WebSocket andrebbe fatta con il token
    let ws_url = format!(
        "ws://127.0.0.1:3000/groups/{}/chat?user_id={}",
        group.id, user_id
    );
    let ws_stream = match connect_async(&ws_url).await {
        Ok((stream, _)) => stream,
        Err(e) => return (None, FromBackend::Error(format!("Impossibile connettersi: {}", e))),
    };

    let (mut write, mut read) = ws_stream.split();
    let (tx, mut rx) = mpsc::channel::<WsMessage>(32);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(msg).await.is_err() {
                break;
            }
        }
    });

    let ui_tx = from_backend_tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            if let WsMessage::Text(text) = msg {
                if let Ok(server_msg) = serde_json::from_str::<WsServerMessage>(&text) {
                    if ui_tx.send(FromBackend::NewMessage(server_msg)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    (Some(tx), FromBackend::GroupJoined(group))
}

// --- Funzione Main ---

fn main() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Ruggine Client",
        native_options,
        Box::new(|cc| Ok(Box::new(RuggineApp::new(cc)))),
    )
}