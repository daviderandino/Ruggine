use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, Rounding, Stroke, Vec2};
use futures_util::{stream::StreamExt, SinkExt};
use reqwest::{header, Client as HttpClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

const API_BASE_URL: &str = "http://127.0.0.1:3000";

// --- Data Structures ---

#[derive(Deserialize, Debug, Clone)]
struct User {
    id: Uuid,
    username: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
struct Group {
    id: Uuid,
    name: String,
}

#[derive(Deserialize, Debug, Clone)]
struct Invitation {
    id: Uuid,
    group_name: String,
    inviter_username: String,
}

#[derive(Serialize)]
struct WsClientMessage {
    content: String,
}

#[derive(Deserialize, Debug, Clone)]
struct WsServerMessage {
    sender_id: Uuid,
    sender_username: String,
    content: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
    user: User,
    groups: Vec<Group>,
}

// --- Messages between UI and Backend Thread ---

enum ToBackend {
    Register(String, String),
    Login(String, String),
    CreateGroup(String),
    JoinGroup(Group),
    LeaveGroup(Uuid),
    InviteUser(Uuid, String),
    SendMessage(Uuid, String),
    FetchInvitations,
    AcceptInvitation(Uuid),
    DeclineInvitation(Uuid),
    FetchGroupMessages(Uuid),
}

#[derive(Debug)]
enum FromBackend {
    LoggedIn(User, String, Vec<Group>),
    Registered,
    GroupJoined(Group),
    GroupLeft(Uuid),
    NewMessage(Uuid, WsServerMessage),
    Info(String),
    Error(String),
    InvitationsFetched(Vec<Invitation>),
    InvitationDeclined(Uuid),
    GroupCreated(Group),
    GroupMessagesFetched(Uuid, Vec<WsServerMessage>),
}

#[derive(PartialEq)]
enum AuthState {
    Login,
    Register,
}

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
    messages: HashMap<Uuid, Vec<WsServerMessage>>,
    pending_invitations: Vec<Invitation>,
    last_invitation_fetch: Instant,
    to_backend_tx: Sender<ToBackend>,
    from_backend_rx: Receiver<FromBackend>,
    _runtime: Runtime,
}

impl RuggineApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (to_backend_tx, mut to_backend_rx) = mpsc::channel(32);
        let (from_backend_tx, from_backend_rx) = mpsc::channel(32);

        configure_styles(&cc.egui_ctx);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let egui_ctx = cc.egui_ctx.clone();
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
                    ToBackend::CreateGroup(group_name) => {
                        match handle_create_group(&client, group_name).await {
                            Ok(group) => {
                                if let Some(token) = &current_token {
                                    if let Ok(ws_tx) = handle_join_group(group.clone(), token.clone(), from_backend_tx.clone()).await {
                                        ws_senders.insert(group.id, ws_tx);
                                    }
                                    let _ = from_backend_tx.send(FromBackend::GroupCreated(group)).await;
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
            messages: HashMap::new(),
            pending_invitations: Vec::new(),
            last_invitation_fetch: Instant::now() - Duration::from_secs(60),
            to_backend_tx,
            from_backend_rx,
            _runtime: runtime,
        }
    }
}

// --- UI Logic ---
impl eframe::App for RuggineApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_backend_messages();
        if self.current_user.is_some() {
            if self.last_invitation_fetch.elapsed() > Duration::from_secs(15) {
                self.to_backend_tx.try_send(ToBackend::FetchInvitations).ok();
                self.last_invitation_fetch = Instant::now();
            }
            self.draw_main_view(ctx);
        } else {
            self.draw_auth_view(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

impl RuggineApp {
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
            }
        }
    }

    fn draw_auth_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.2);
                ui.heading("Benvenuto in Ruggine");
                ui.add_space(20.0);
                Frame::none().inner_margin(Margin::same(20.0)).fill(ui.style().visuals.widgets.noninteractive.bg_fill).rounding(Rounding::same(8.0)).show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.vertical_centered_justified(|ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.auth_state, AuthState::Login, "Login");
                            ui.selectable_value(&mut self.auth_state, AuthState::Register, "Registrati");
                        });
                        ui.add_space(15.0);
                        ui.label("Username");
                        ui.text_edit_singleline(&mut self.username_input);
                        ui.add_space(10.0);
                        ui.label("Password");
                        ui.add(egui::TextEdit::singleline(&mut self.password_input).password(true));
                        ui.add_space(20.0);
                        let button_text = if self.auth_state == AuthState::Login { "Login" } else { "Registrati" };
                        if ui.button(button_text).clicked() {
                            let action = if self.auth_state == AuthState::Login {
                                ToBackend::Login(self.username_input.clone(), self.password_input.clone())
                            } else {
                                ToBackend::Register(self.username_input.clone(), self.password_input.clone())
                            };
                            let _ = self.to_backend_tx.try_send(action);
                        }
                    });
                });
                self.draw_info_error_messages(ui);
            });
        });
    }

    fn draw_main_view(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("side_panel").min_width(250.0).default_width(250.0).show(ctx, |ui| {
            ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                ui.add_space(10.0);
                ui.heading(format!("Ciao, {}!", self.current_user.as_ref().unwrap().username));
                ui.add_space(20.0);

                // Sezione per la creazione di un nuovo gruppo
                Frame::none().inner_margin(Margin::symmetric(10.0, 15.0)).show(ui, |ui| {
                    ui.label("Crea un nuovo Gruppo");
                    ui.text_edit_singleline(&mut self.create_group_input);
                    if ui.button("➕ Crea").clicked() {
                        if !self.create_group_input.is_empty() {
                           let _ = self.to_backend_tx.try_send(ToBackend::CreateGroup(self.create_group_input.clone()));
                           self.create_group_input.clear();
                        }
                    }
                });

                ui.separator();
                
                // Lista dei gruppi a cui l'utente appartiene
                ui.heading("I Miei Gruppi");
                // Modifica qui: usa `ui.push_id` per creare un contesto con ID univoco per lo ScrollArea
                ui.push_id("my_groups_scroll_area", |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for group in self.user_groups.clone() {
                            let is_selected = self.selected_group_id == Some(group.id);
                            if ui.selectable_value(&mut self.selected_group_id, Some(group.id), format!("# {}", group.name)).clicked() {
                                self.to_backend_tx.try_send(ToBackend::FetchGroupMessages(group.id)).ok();
                            }
                            if is_selected {
                                ui.add_space(5.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(10.0);
                                    if ui.button("❌ Esci").clicked() {
                                        let _ = self.to_backend_tx.try_send(ToBackend::LeaveGroup(group.id));
                                    }
                                    ui.add_space(10.0);
                                    ui.label("Invita:");
                                    ui.text_edit_singleline(&mut self.invite_user_input);
                                    if ui.button("✉ Invia Invito").clicked() {
                                        if !self.invite_user_input.is_empty() {
                                            self.to_backend_tx.try_send(ToBackend::InviteUser(group.id, self.invite_user_input.clone())).ok();
                                            self.invite_user_input.clear();
                                        }
                                    }
                                });
                            }
                        }
                    });
                });
                
                ui.separator();
                self.draw_invitations_section(ui);
                self.draw_info_error_messages(ui);
            });
        });

        if let Some(selected_id) = self.selected_group_id {
            let selected_group = self.user_groups.iter().find(|g| g.id == selected_id).cloned();
            if let Some(group) = selected_group {
                egui::TopBottomPanel::bottom("chat_input_panel").resizable(false).min_height(40.0).show(ctx, |ui| {
                    ui.separator();
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let text_edit_response = ui.add_sized(ui.available_size(), egui::TextEdit::singleline(&mut self.chat_message_input).hint_text(format!("Messaggio in #{}", group.name)).frame(false));
                        if text_edit_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.chat_message_input.is_empty() {
                            if let Some(group_id) = self.selected_group_id {
                                let _ = self.to_backend_tx.try_send(ToBackend::SendMessage(group_id, self.chat_message_input.clone()));
                            }
                            self.chat_message_input.clear();
                            text_edit_response.request_focus();
                        }
                    });
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| { ui.heading(format!("# {}", group.name)); });
                    ui.separator();
                    egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false; 2]).show(ui, |ui| {
                        ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                            ui.add_space(10.0);
                            if let Some(messages) = self.messages.get(&selected_id) {
                                for msg in messages { self.draw_message_bubble(ui, msg); }
                            }
                        });
                    });
                });
            }
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("Crea un gruppo o accetta un invito per iniziare.");
                });
            });
        }
    }

    fn draw_invitations_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.heading("Inviti Pendenti");
        ui.add_space(5.0);
        if self.pending_invitations.is_empty() {
            ui.label("Nessun invito.");
        } else {
            // Modifica qui: usa `ui.push_id` per creare un contesto con ID univoco per lo ScrollArea
            ui.push_id("invitations_scroll_area", |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                    for invitation in self.pending_invitations.clone() {
                        Frame::none().inner_margin(Margin::same(10.0)).fill(ui.style().visuals.widgets.noninteractive.bg_fill).rounding(Rounding::same(5.0)).show(ui, |ui| {
                            ui.label(egui::RichText::new(&invitation.group_name).strong());
                            ui.label(format!("Da: {}", invitation.inviter_username));
                            ui.horizontal(|ui| {
                                if ui.button("✅ Accetta").clicked() {
                                    self.to_backend_tx.try_send(ToBackend::AcceptInvitation(invitation.id)).ok();
                                }
                                if ui.button("❌ Rifiuta").clicked() {
                                    self.to_backend_tx.try_send(ToBackend::DeclineInvitation(invitation.id)).ok();
                                }
                            });
                        });
                        ui.add_space(5.0);
                    }
                });
            });
        }
    }

    fn draw_message_bubble(&self, ui: &mut egui::Ui, msg: &WsServerMessage) {
        if msg.sender_id.is_nil() {
            ui.add_space(4.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(&msg.content)
                        .italics()
                        .color(egui::Color32::GRAY),
                );
            });
            ui.add_space(4.0);
            return;
        }

        let is_my_message = self.current_user.as_ref().unwrap().id == msg.sender_id;
        let layout = if is_my_message { Layout::right_to_left(Align::TOP) } else { Layout::left_to_right(Align::TOP) };
        
        ui.with_layout(layout, |ui| {
             Frame::none()
                .inner_margin(Margin::symmetric(12.0, 8.0))
                .rounding(Rounding { nw: 12.0, ne: 12.0, sw: if is_my_message { 2.0 } else { 12.0 }, se: if is_my_message { 12.0 } else { 2.0 } })
                .fill(if is_my_message { egui::Color32::from_rgb(136, 192, 208) } else { ui.style().visuals.widgets.noninteractive.bg_fill })
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width() * 0.7);
                    ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                        if !is_my_message {
                             ui.label(egui::RichText::new(&msg.sender_username).strong().color(egui::Color32::from_rgb(202, 211, 245)));
                        }
                        ui.label(egui::RichText::new(&msg.content).color(if is_my_message { egui::Color32::from_gray(10) } else { egui::Color32::from_gray(220) }).size(15.0));
                    });
                });
        });
        ui.add_space(4.0);
    }

    fn draw_info_error_messages(&self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        if let Some(info) = &self.info_message {
            ui.label(egui::RichText::new(info).color(egui::Color32::from_rgb(166, 209, 137)));
        }
        if let Some(err) = &self.error_message {
            ui.label(egui::RichText::new(err).color(egui::Color32::from_rgb(237, 135, 150)));
        }
    }
}

fn configure_styles(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let visuals = &mut style.visuals;

    // --- Oceanic Dark Palette ---
    let bg_main      = Color32::from_rgb(26, 27, 38);       // Dark Blue
    let bg_secondary = Color32::from_rgb(36, 40, 59);       // Lighter Blue/Gray
    let bg_hover     = Color32::from_rgb(50, 56, 80);       // Hover Blue
    let fg_text      = Color32::from_rgb(192, 202, 245);    // Light Lavender
    let accent_color = Color32::from_rgb(42, 195, 222);     // Vibrant Cyan
    let widget_stroke = Stroke::new(1.0, Color32::from_rgb(65, 72, 104));

    visuals.dark_mode = true;
    visuals.override_text_color = Some(fg_text);
    visuals.window_rounding = Rounding::same(8.0);
    visuals.window_fill = bg_main;
    visuals.window_stroke = Stroke::new(1.0, bg_secondary);

    visuals.widgets.noninteractive.bg_fill = bg_main;
    visuals.widgets.noninteractive.bg_stroke = widget_stroke;
    visuals.widgets.noninteractive.rounding = Rounding::same(4.0);

    visuals.widgets.inactive.bg_fill = bg_secondary;
    visuals.widgets.inactive.rounding = Rounding::same(4.0);

    visuals.widgets.hovered.bg_fill = bg_hover;
    visuals.widgets.hovered.rounding = Rounding::same(4.0);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent_color);

    visuals.widgets.active.bg_fill = accent_color;
    visuals.widgets.active.rounding = Rounding::same(4.0);
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, Color32::BLACK);

    visuals.selection.bg_fill = accent_color.linear_multiply(0.4);
    visuals.selection.stroke = Stroke::new(1.0, accent_color);

    style.spacing.button_padding = Vec2::new(10.0, 8.0);
    ctx.set_style(style);
}


// --- Network Logic ---
async fn handle_register(client: &HttpClient, username: String, password: String) -> FromBackend {
    if username.is_empty() || password.is_empty() { return FromBackend::Error("Username e password non possono essere vuoti.".into()); }
    let payload = serde_json::json!({ "username": username, "password": password });
    match client.post(format!("{}/users/register", API_BASE_URL)).json(&payload).send().await {
        Ok(res) if res.status().is_success() => FromBackend::Registered,
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into())),
        Err(_) => FromBackend::Error("Impossibile connettersi al server.".into()),
    }
}

async fn handle_login(
    username: String,
    password: String,
) -> Result<(FromBackend, HttpClient), FromBackend> {
    if username.is_empty() || password.is_empty() {
        return Err(FromBackend::Error("Username e password non possono essere vuoti.".into()));
    }
    let unauthed_client = HttpClient::new();
    let payload = serde_json::json!({ "username": username, "password": password });

    match unauthed_client
        .post(format!("{}/users/login", API_BASE_URL))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            let login_res = res
                .json::<LoginResponse>()
                .await
                .map_err(|_| FromBackend::Error("Errore risposta server.".into()))?;

            let mut headers = header::HeaderMap::new();
            headers.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&format!("Bearer {}", login_res.token)).unwrap(),
            );
            let authenticated_client = HttpClient::builder().default_headers(headers).build().unwrap();

            Ok((
                FromBackend::LoggedIn(login_res.user, login_res.token, login_res.groups),
                authenticated_client,
            ))
        }
        Ok(res) => Err(FromBackend::Error(
            res.text()
                .await
                .unwrap_or_else(|_| "Username o password non validi".into()),
        )),
        Err(_) => Err(FromBackend::Error(
            "Impossibile connettersi al server.".into(),
        )),
    }
}

async fn handle_create_group(client: &HttpClient, name: String) -> Result<Group, FromBackend> {
    if name.is_empty() { return Err(FromBackend::Error("Il nome del gruppo non può essere vuoto.".into())); }
    let payload = serde_json::json!({ "name": name });
    match client.post(format!("{}/groups", API_BASE_URL)).json(&payload).send().await {
        Ok(res) if res.status().is_success() => {
            res.json::<Group>().await.map_err(|_| FromBackend::Error("Errore decodifica gruppo creato.".into()))
        }
        Ok(res) => Err(FromBackend::Error(res.text().await.unwrap_or_default())),
        Err(_) => Err(FromBackend::Error("Errore di connessione.".into())),
    }
}

async fn handle_leave_group(client: &HttpClient, group_id: Uuid) -> FromBackend {
    match client.delete(format!("{}/groups/{}/leave", API_BASE_URL, group_id)).send().await {
        Ok(res) if res.status().is_success() => FromBackend::GroupLeft(group_id),
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_else(|_| "Errore durante l'uscita dal gruppo.".into())),
        Err(_) => FromBackend::Error("Errore di connessione.".into()),
    }
}

async fn handle_invite(
    client: &HttpClient,
    group_id: Uuid,
    username_to_invite: String,
) -> FromBackend {
    if username_to_invite.is_empty() {
        return FromBackend::Error("Devi specificare un utente da invitare.".into());
    }

    let user_to_invite = match client
        .get(format!("{}/users/by_username/{}", API_BASE_URL, username_to_invite))
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => res.json::<User>().await.unwrap(),
        _ => return FromBackend::Error(format!("Utente '{}' non trovato.", username_to_invite)),
    };

    let payload = serde_json::json!({ "user_to_invite_id": user_to_invite.id });

    match client
        .post(format!("{}/groups/{}/invite", API_BASE_URL, group_id))
        .json(&payload)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            FromBackend::Info(format!("Invito inviato a {}.", username_to_invite))
        }
        Ok(res) => FromBackend::Error(
            res.text()
                .await
                .unwrap_or_else(|_| "Errore durante l'invito.".into()),
        ),
        Err(_) => FromBackend::Error("Errore di connessione durante l'invito.".into()),
    }
}


async fn handle_fetch_invitations(client: &HttpClient) -> FromBackend {
    match client.get(format!("{}/invitations", API_BASE_URL)).send().await {
        Ok(res) if res.status().is_success() => match res.json::<Vec<Invitation>>().await {
            Ok(invitations) => FromBackend::InvitationsFetched(invitations),
            Err(_) => FromBackend::Error("Errore nel decodificare gli inviti.".into()),
        },
        _ => FromBackend::Error("Impossibile recuperare gli inviti.".into()),
    }
}

async fn handle_accept_invitation(client: &HttpClient, id: Uuid) -> Result<Group, FromBackend> {
    match client.post(format!("{}/invitations/{}/accept", API_BASE_URL, id)).send().await {
        Ok(res) if res.status().is_success() => res.json::<Group>().await.map_err(|_| FromBackend::Error("Errore decodifica gruppo.".into())),
        Ok(res) => Err(FromBackend::Error(res.text().await.unwrap_or_default())),
        Err(_) => Err(FromBackend::Error("Errore di connessione.".into())),
    }
}

async fn handle_decline_invitation(client: &HttpClient, id: Uuid) -> FromBackend {
    match client.post(format!("{}/invitations/{}/decline", API_BASE_URL, id)).send().await {
        Ok(res) if res.status().is_success() => FromBackend::InvitationDeclined(id),
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_default()),
        Err(_) => FromBackend::Error("Errore di connessione.".into()),
    }
}

async fn handle_fetch_group_messages(client: &HttpClient, group_id: Uuid) -> FromBackend {
    match client.get(format!("{}/groups/{}/messages", API_BASE_URL, group_id)).send().await {
        Ok(res) if res.status().is_success() => {
            match res.json::<Vec<WsServerMessage>>().await {
                Ok(messages) => FromBackend::GroupMessagesFetched(group_id, messages),
                Err(_) => FromBackend::Error("Errore nel decodificare la cronologia dei messaggi.".into()),
            }
        },
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_default()),
        Err(_) => FromBackend::Error("Errore di connessione per la cronologia dei messaggi.".into()),
    }
}

async fn handle_join_group(
    group: Group,
    token: String,
    from_backend_tx: Sender<FromBackend>
) -> Result<Sender<WsMessage>, FromBackend> {
    let ws_url = format!("ws://127.0.0.1:3000/groups/{}/chat?token={}", group.id, token);
    let ws_stream = match connect_async(&ws_url).await {
        Ok((stream, _)) => stream,
        Err(e) => return Err(FromBackend::Error(format!("Impossibile connettersi alla chat: {}", e))),
    };
    
    let (mut write, mut read) = ws_stream.split();
    let (tx, mut rx) = mpsc::channel::<WsMessage>(32);
    tokio::spawn(async move { while let Some(msg) = rx.recv().await { if write.send(msg).await.is_err() { break; } } });

    let ui_tx = from_backend_tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            if let WsMessage::Text(text) = msg {
                if let Ok(server_msg) = serde_json::from_str::<WsServerMessage>(&text) {
                    if ui_tx.send(FromBackend::NewMessage(group.id, server_msg)).await.is_err() { break; }
                }
            }
        }
    });
    
    Ok(tx)
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