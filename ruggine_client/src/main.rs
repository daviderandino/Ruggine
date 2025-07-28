use eframe::egui;
use futures_util::{stream::StreamExt, SinkExt};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

const API_BASE_URL: &str = "http://127.0.0.1:3000";

// --- Strutture Dati e Messaggi ---

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

// Messaggi che l'interfaccia invia al task di rete
enum ToBackend {
    Register(String),
    CreateGroup(String, Uuid),         // Includiamo l'ID del creatore
    JoinGroup(String, Uuid),             // Includiamo l'ID dell'utente
    InviteUser(String, String, Uuid), // group_name, username_to_invite, inviter_id
    SendMessage(String),
}

// Messaggi che il task di rete invia all'interfaccia
#[derive(Debug)]
enum FromBackend {
    Registered(User),
    GroupJoined(Group),
    NewMessage(WsServerMessage),
    Info(String),
    Error(String),
}

// --- Stato dell'Applicazione GUI ---

struct RuggineApp {
    // Stato UI
    username_input: String,
    create_group_input: String,
    join_group_input: String,
    invite_user_input: String,
    chat_message_input: String,
    error_message: Option<String>,
    info_message: Option<String>,

    // Stato Applicazione
    current_user: Option<User>,
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
            let client = HttpClient::new();
            let mut ws_sender: Option<Sender<WsMessage>> = None;

            while let Some(action) = to_backend_rx.recv().await {
                match action {
                    ToBackend::Register(username) => {
                        let res = handle_register(&client, username).await;
                        let _ = from_backend_tx.send(res).await;
                    }
                    ToBackend::CreateGroup(group_name, creator_id) => {
                        let res = handle_create_group(&client, group_name, creator_id).await;
                        let _ = from_backend_tx.send(res).await;
                    }
                    ToBackend::JoinGroup(group_name, user_id) => {
                        let (ws_tx, res) =
                            handle_join_group(&client, group_name, user_id, from_backend_tx.clone())
                                .await;
                        ws_sender = ws_tx;
                        let _ = from_backend_tx.send(res).await;
                    }
                    ToBackend::InviteUser(group_name, username_to_invite, inviter_id) => {
                        let res =
                            handle_invite(&client, group_name, username_to_invite, inviter_id)
                                .await;
                        let _ = from_backend_tx.send(res).await;
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
            create_group_input: String::new(),
            join_group_input: String::new(),
            invite_user_input: String::new(),
            chat_message_input: String::new(),
            error_message: None,
            info_message: None,
            current_user: None,
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
            self.draw_register_view(ctx);
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
                FromBackend::Registered(user) => self.current_user = Some(user),
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

    fn draw_register_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Benvenuto in Ruggine");
                ui.add_space(20.0);
                ui.set_max_width(200.0);

                ui.horizontal(|ui| {
                    ui.label("Scegli un username:");
                    ui.text_edit_singleline(&mut self.username_input);
                });
                ui.add_space(10.0);

                if ui.button("Registrati").clicked() {
                    let _ = self.to_backend_tx
                        .try_send(ToBackend::Register(self.username_input.clone()));
                }
                self.draw_info_error_messages(ui);
            });
        });
    }

    fn draw_main_view(&mut self, ctx: &egui::Context) {
        let user_id = self.current_user.as_ref().unwrap().id;

        egui::SidePanel::left("side_panel")
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Gruppi");

                ui.separator();
                ui.label("Crea un nuovo gruppo:");
                ui.text_edit_singleline(&mut self.create_group_input);
                if ui.button("Crea").clicked() {
                    let _ = self.to_backend_tx.try_send(ToBackend::CreateGroup(
                        self.create_group_input.clone(),
                        user_id,
                    ));
                }

                ui.separator();
                ui.label("Unisciti a un gruppo:");
                ui.text_edit_singleline(&mut self.join_group_input);
                if ui.button("Join").clicked() {
                    let _ = self.to_backend_tx.try_send(ToBackend::JoinGroup(
                        self.join_group_input.clone(),
                        user_id,
                    ));
                }

                if let Some(group) = &self.current_group {
                    ui.separator();
                    ui.label(format!("Invita in '{}':", group.name));
                    ui.text_edit_singleline(&mut self.invite_user_input);
                    if ui.button("Invita").clicked() {
                        let _ = self.to_backend_tx.try_send(ToBackend::InviteUser(
                            group.name.clone(),
                            self.invite_user_input.clone(),
                            user_id,
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
                        let _ = self.to_backend_tx
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
async fn handle_register(client: &HttpClient, username: String) -> FromBackend {
    if username.is_empty() {
        return FromBackend::Error("L'username non può essere vuoto.".into());
    }
    let payload = serde_json::json!({ "username": username });
    match client
        .post(format!("{}/users/register", API_BASE_URL))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => res
            .json::<User>()
            .await
            .map(FromBackend::Registered)
            .unwrap_or_else(|_| FromBackend::Error("Errore risposta server.".into())),
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_else(|_| "Username già in uso.".into())),
        Err(_) => FromBackend::Error("Impossibile connettersi al server.".into()),
    }
}

async fn handle_create_group(client: &HttpClient, name: String, creator_id: Uuid) -> FromBackend {
    if name.is_empty() {
        return FromBackend::Error("Il nome del gruppo non può essere vuoto.".into());
    }
    let payload = serde_json::json!({ "name": name.clone(), "creator_id": creator_id });
    match client
        .post(format!("{}/groups", API_BASE_URL))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => FromBackend::Info(format!("Gruppo '{}' creato.", name)),
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