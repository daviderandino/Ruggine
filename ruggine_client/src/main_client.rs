use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, Rounding, Stroke, Vec2};
use futures_util::{stream::StreamExt, SinkExt};
use reqwest::{header, Client as HttpClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::format;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;
use reqwest::StatusCode;

const API_BASE_URL: &str = "http://127.0.0.1:3000";
const REFRESH_RATE:u64 = 50; //ms

// Dichiarazione moduli
mod draw;
mod handlers_client;
pub mod models_client;
use crate::draw::*;
use crate::handlers_client::*;
use crate::models_client::*;

// --- Application State ---
struct RuggineApp {
    username_input: String,
    password_input: String,
    create_group_input: String,
    invite_user_input: String,
    chat_message_input: String,
    error_message: Option<String>,
    info_message: Option<String>,
    auth_state: AuthState,
    current_user: Option<User>,
    auth_token: Option<String>,
    user_groups: Vec<Group>,
    selected_group_id: Option<Uuid>,
    selected_group_members: Option<Vec<User>>,
    messages: HashMap<Uuid, Vec<WsServerMessage>>,
    pending_invitations: Vec<Invitation>,
    last_invitation_fetch: Instant,
    to_backend_tx: Sender<ToBackend>,
    from_backend_rx: Receiver<FromBackend>,
    _runtime: Runtime,
}
impl RuggineApp {
fn new(cc: &eframe::CreationContext<'_>) -> Self {
    /*to_backend transmitter e from_backend reciver finiranno salvati nello stato dell'applicazione

    to_backend reciver e from_backend transmitter vengono dati al "thread handler" generato sotto con tokio
    
    Il Thread handler riceve le richieste generate durante l'uso della UI sul canale to_backend.
    Con l'opportuno handler, il thread chiama il back-end e ne attende la risposta.
    Ottenuta la risposta, il thread handler trasmette sul canale from_backend.
    Ogni 50ms la funzione update chiama la funzione handle_backend_messages che svuota il canale from_backend
    e modifica lo stato dell'applicazione.

    La UI viene ridisegnata seguendo lo stato dell'applicazione e attraverso richieste di fetch al backend.
    */
    let (to_backend_tx, mut to_backend_rx) = mpsc::channel(32);
    let (from_backend_tx, from_backend_rx) = mpsc::channel(32);

    configure_styles(&cc.egui_ctx);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let egui_ctx = cc.egui_ctx.clone();
    
    //Thread handler
    runtime.spawn(async move {
        let mut client = HttpClient::new();
        let mut ws_senders: HashMap<Uuid, Sender<WsMessage>> = HashMap::new();
        let mut _current_user: Option<User> = None;
        let mut current_token: Option<String> = None;

        while let Some(action) = to_backend_rx.recv().await {
        match action {
            ToBackend::Register(username, password) => {
                let res = handle_register(&client, username, password).await;
                let _ = from_backend_tx.send(res).await;
            }
            ToBackend::Login(username, password) => {
                match handle_login(username, password).await {
                    Ok((from_backend_msg, authenticated_client)) => {
                        client = authenticated_client;
                        if let FromBackend::LoggedIn(ref user, ref token, ref groups) = from_backend_msg {
                            
                            _current_user = Some(user.clone());
                            current_token = Some(token.clone());

                            // Chiudi le connessioni WebSocket precedenti
                            for (_, sender) in ws_senders.drain() {
                                let _ = sender.send(WsMessage::Close(None)).await;
                            }

                            // Subscribe to all groups upon login
                            for group in groups {
                                if let Some(token) = &current_token {
                                    if let Ok(ws_tx) = handle_join_group(group.clone(), token.clone(), from_backend_tx.clone()).await {
                                        ws_senders.insert(group.id, ws_tx);
                                    }
                                }
                            }
                        }
                        let _ = from_backend_tx.send(from_backend_msg).await;
                    }
                    Err(e) => {
                        let _ = from_backend_tx.send(e).await;
                    }
                }
            }
            ToBackend::Logout => {
                // Chiudi correttamente ogni WebSocket
                for (_, sender) in ws_senders.drain() {
                    let _ = sender.send(WsMessage::Close(None)).await;
                }

                _current_user = None;
                current_token = None;
                client = HttpClient::new();

                let _ = from_backend_tx.send(FromBackend::Info("Logout effettuato.".into())).await;
            }
            ToBackend::CreateGroup(group_name) => {
                match handle_create_group(&client, group_name).await {
                    Ok(group) => {
                        if let Some(token) = &current_token {
                            if let Ok(ws_tx) = handle_join_group(group.clone(), token.clone(), from_backend_tx.clone()).await {
                                ws_senders.insert(group.id, ws_tx);
                            }
                            let _ = from_backend_tx.send(FromBackend::GroupCreated(group.clone())).await;
                            let users = handle_fetch_group_members(&client, group.id).await;
                            let _ = from_backend_tx.send(users).await;
                        }
                    }
                    Err(e) => {
                        let _ = from_backend_tx.send(e).await;
                    }
                }
            }
            ToBackend::JoinGroup(group) => {
                    if let Some(token) = &current_token {
                        match handle_join_group(group.clone(), token.clone(), from_backend_tx.clone()).await {
                        Ok(ws_tx) => {
                            ws_senders.insert(group.id, ws_tx);
                            let _ = from_backend_tx.send(FromBackend::GroupJoined(group.clone())).await;
                            let _ = from_backend_tx.send(FromBackend::Info(format!("Entrato in '{}'", group.name))).await;
                        }
                        Err(e) => {
                            let _ = from_backend_tx.send(e).await;
                        }
                    }
                }
            }
            ToBackend::LeaveGroup(group_id) => {
                let res = handle_leave_group(&client, group_id).await;
                if let FromBackend::GroupLeft(id) = res {
                    if let Some(sender) = ws_senders.remove(&id) {
                        let _ = sender.send(WsMessage::Close(None)).await;
                    }
                }
                let _ = from_backend_tx.send(res).await;
            }
            ToBackend::InviteUser(group_id, username_to_invite) => {
                let res = handle_invite(&client, group_id, username_to_invite).await;
                let _ = from_backend_tx.send(res).await;
            }
            ToBackend::SendMessage(group_id, content) => {
                if let Some(sender) = ws_senders.get(&group_id) {
                    let msg = WsClientMessage { content };
                    let json_msg = serde_json::to_string(&msg).unwrap();
                    if sender.send(WsMessage::Text(json_msg)).await.is_err() {
                        let _ = from_backend_tx.send(FromBackend::Error("Connessione persa.".into())).await;
                    }
                }
            }
            ToBackend::FetchInvitations => {
                let res = handle_fetch_invitations(&client).await;
                let _ = from_backend_tx.send(res).await;
            }
            ToBackend::AcceptInvitation(id) => {
                    match handle_accept_invitation(&client, id).await {
                    Ok(group) => {
                        if let Some(token) = &current_token {
                            if let Ok(ws_tx) = handle_join_group(group.clone(), token.clone(), from_backend_tx.clone()).await {
                                ws_senders.insert(group.id, ws_tx);
                            }
                            let _ = from_backend_tx.send(FromBackend::GroupJoined(group)).await;
                        }
                    }
                    Err(e) => {
                        let _ = from_backend_tx.send(e).await;
                    }
                }
            }
            ToBackend::DeclineInvitation(id) => {
                let res = handle_decline_invitation(&client, id).await;
                let _ = from_backend_tx.send(res).await;
            }
            ToBackend::FetchGroupMessages(group_id) => {
                let res = handle_fetch_group_messages(&client, group_id).await;
                let _ = from_backend_tx.send(res).await;
            }
            ToBackend::FetchGroupMembers(group_id) =>{
                let res = handle_fetch_group_members(&client, group_id).await;
                let _ = from_backend_tx.send(res).await;
            }
        }
        egui_ctx.request_repaint();
    }
    });

    Self {
        username_input: String::new(),
        password_input: String::new(),
        create_group_input: String::new(),
        invite_user_input: String::new(),
        chat_message_input: String::new(),
        error_message: None,
        info_message: None,
        auth_state: AuthState::Login,
        current_user: None,
        auth_token: None,
        user_groups: Vec::new(),
        selected_group_id: None,
        selected_group_members:None,
        messages: HashMap::new(),
        pending_invitations: Vec::new(),
        last_invitation_fetch: Instant::now(),
        to_backend_tx,
        from_backend_rx,
        _runtime: runtime,
    }
}
//Ogni 50ms viene chiamata dentro egui::App::update e svuota il canale "from_backend" aggiornando lo stato dell'applicazione
 fn handle_backend_messages(&mut self) {
    while let Ok(msg) = self.from_backend_rx.try_recv() {
        self.error_message = None;
        self.info_message = None;
        match msg {
            FromBackend::LoggedIn(user, token, groups) => {
                                    self.current_user = Some(user);
                                    self.auth_token = Some(token);
                                    self.user_groups = groups.clone();
                                    if let Some(first_group) = groups.get(0) {
                                        self.selected_group_id = Some(first_group.id);
                                        self.to_backend_tx.try_send(ToBackend::FetchGroupMessages(first_group.id)).ok();
                                        self.to_backend_tx.try_send(ToBackend::FetchGroupMembers(first_group.id)).ok();

                                    }
                                }
            FromBackend::Registered => {
                                    self.info_message = Some("Registrazione avvenuta! Ora puoi effettuare il login.".into());
                                    self.auth_state = AuthState::Login;
                                }
            FromBackend::GroupJoined(group) => {
                                    self.info_message = Some(format!("Entrato in '{}'", group.name));
                                    self.selected_group_id = Some(group.id);
                                    // Rimuovi il gruppo se esiste già e aggiungi la nuova istanza per aggiornare
                                    self.user_groups.retain(|g| g.id != group.id);
                                    self.user_groups.push(group);
                                    self.to_backend_tx.try_send(ToBackend::FetchGroupMessages(self.selected_group_id.unwrap())).ok();
                                    self.to_backend_tx.try_send(ToBackend::FetchGroupMembers(self.selected_group_id.unwrap())).ok();

                                }
            FromBackend::GroupCreated(group) => {
                                    self.info_message = Some(format!("Gruppo '{}' creato.", group.name));
                                    self.selected_group_id = Some(group.id);
                                    self.user_groups.push(group);
                                    self.messages.insert(self.selected_group_id.unwrap(), vec![]);
                                }
            FromBackend::GroupLeft(group_id) => {
                                    self.info_message = Some(format!("Hai lasciato un gruppo."));
                                    self.user_groups.retain(|g| g.id != group_id);
                                    self.messages.remove(&group_id);
                                    if self.selected_group_id == Some(group_id) {
                                        self.selected_group_id = self.user_groups.get(0).map(|g| g.id);
                                        if let Some(id) = self.selected_group_id {
                                                self.to_backend_tx.try_send(ToBackend::FetchGroupMessages(id)).ok();
                                                self.to_backend_tx.try_send(ToBackend::FetchGroupMembers(id)).ok();

                                        }
                                    }
                                }
            FromBackend::NewMessage(group_id, msg) => {
                                    self.messages.entry(group_id).or_default().push(msg);
                                },
            FromBackend::Error(err) => self.error_message = Some(err),
            FromBackend::Info(info) => self.info_message = Some(info),
            FromBackend::InvitationsFetched(invitations) => {
                                    self.pending_invitations = invitations;
                                }
            FromBackend::InvitationDeclined(id) => {
                                    self.pending_invitations.retain(|inv| inv.id != id);
                                    self.info_message = Some("Invito rifiutato.".into());
                                }
            FromBackend::GroupMessagesFetched(group_id, history) => {
                                    self.messages.insert(group_id, history);
                                }
            FromBackend::GroupMembersFetched(_, members) => self.selected_group_members = Some(members),
            FromBackend::GroupMembersChanged =>{  
                                if let Some(id) = self.selected_group_id {
                                    self.to_backend_tx.try_send(ToBackend::FetchGroupMessages(id)).ok();
                                    self.to_backend_tx.try_send(ToBackend::FetchGroupMembers(id)).ok();

                                }},
        }
    }
}

}
// --- UI Logic ---
impl eframe::App for RuggineApp {
    //Chiamata ogni REFRESH_RATE ms per aggiornare stato dell'applicazione e UI
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        //Modifico stato di Ruggine APP
        self.handle_backend_messages();

        // Ridisegno la UI
        if self.current_user.is_some() {
            if self.last_invitation_fetch.elapsed() > Duration::from_secs(5) {
                self.to_backend_tx.try_send(ToBackend::FetchInvitations).ok();
                self.last_invitation_fetch = Instant::now();
            }
            self.draw_main_view(ctx);
        } else {
            self.draw_auth_view(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(REFRESH_RATE));
    }
}

fn main() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Ruggine Client",
        native_options,
        Box::new(|cc| Ok(Box::new(RuggineApp::new(cc)))),
    )
}