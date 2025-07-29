use eframe::egui::{self, Align, Frame, Layout, Rounding, Stroke, Vec2}; // Importazioni aggiuntive per lo stile
use eframe::egui::Margin;
use futures_util::{stream::StreamExt, SinkExt};
use reqwest::{header, Client as HttpClient};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

const API_BASE_URL: &str = "http://127.0.0.1:3000";

// --- Strutture Dati e Messaggi (invariate) ---

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
    sender_id: Uuid, // Aggiunto per distinguere i messaggi dell'utente
    sender_username: String,
    content: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

enum ToBackend {
    Register(String, String),
    Login(String, String),
    CreateGroup(String),
    JoinGroup(String),
    InviteUser(String, String),
    SendMessage(String),
}

#[derive(Debug)]
enum FromBackend {
    LoggedIn(User, String),
    Registered,
    GroupJoined(Group, Uuid), // Passiamo anche l'user_id per la UI
    NewMessage(WsServerMessage),
    Info(String),
    Error(String),
}

#[derive(PartialEq)]
enum AuthState {
    Login,
    Register,
}

// --- Stato dell'Applicazione GUI (invariato) ---

struct RuggineApp {
    username_input: String,
    password_input: String,
    create_group_input: String,
    join_group_input: String,
    invite_user_input: String,
    chat_message_input: String,
    error_message: Option<String>,
    info_message: Option<String>,
    auth_state: AuthState,
    current_user: Option<User>,
    auth_token: Option<String>,
    current_group: Option<Group>,
    messages: Vec<WsServerMessage>,
    to_backend_tx: Sender<ToBackend>,
    from_backend_rx: Receiver<FromBackend>,
    _runtime: Runtime,
}

impl RuggineApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (to_backend_tx, mut to_backend_rx) = mpsc::channel(32);
        let (from_backend_tx, from_backend_rx) = mpsc::channel(32);

        // NUOVO: Applichiamo lo stile moderno all'avvio
        configure_styles(&cc.egui_ctx);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        
        // La logica di rete rimane invariata...
        runtime.spawn(async move {
            let mut client = HttpClient::new();
            let mut ws_sender: Option<Sender<WsMessage>> = None;
            let mut current_user: Option<User> = None;
            let mut current_token: Option<String> = None;

            while let Some(action) = to_backend_rx.recv().await {
                match action {
                    ToBackend::Register(username, password) => {
                        let res = handle_register(&client, username, password).await;
                        let _ = from_backend_tx.send(res).await;
                    }
                    ToBackend::Login(username, password) => {
                        let res = handle_login(&client, username.clone(), password).await;
                        if let Ok(FromBackend::LoggedIn(ref user, ref token)) = res {
                            current_user = Some(user.clone());
                            current_token = Some(token.clone());
                            
                            let mut headers = header::HeaderMap::new();
                            headers.insert(
                                header::AUTHORIZATION,
                                header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
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
                         if let (Some(user), Some(token)) = (&current_user, &current_token) {
                            let (ws_tx, res) = handle_join_group(
                                &client,
                                group_name,
                                user.clone(), // Passiamo l'intero utente
                                token.clone(),
                                from_backend_tx.clone(),
                            )
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

// --- Logica di Disegno UI (MODIFICATA) ---

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
                    self.auth_state = AuthState::Login;
                }
                FromBackend::GroupJoined(group, _user_id) => {
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
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() * 0.2); // Spazio verticale
                ui.heading("Benvenuto in Ruggine");
                ui.add_space(20.0);

                Frame::none()
                    .inner_margin(Margin::same(20.0))
                    .fill(ui.style().visuals.widgets.noninteractive.bg_fill)
                    .rounding(Rounding::same(8.0))
                    .show(ui, |ui| {
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
    // Il pannello laterale rimane invariato
    egui::SidePanel::left("side_panel")
        .min_width(250.0)
        .default_width(250.0)
        .show(ctx, |ui| {
            ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                ui.add_space(10.0);
                ui.heading(format!("Ciao, {}!", self.current_user.as_ref().unwrap().username));
                ui.add_space(20.0);
                
                // Le chiamate a `draw_group_section` qui sono state corrette per passare il sender
                // La tua versione del codice aveva già questa correzione, la includo per completezza.
                Self::draw_group_section(ui, "Crea Gruppo", &mut self.create_group_input, &self.to_backend_tx, ToBackend::CreateGroup);
                Self::draw_group_section(ui, "Unisciti a un Gruppo", &mut self.join_group_input, &self.to_backend_tx, ToBackend::JoinGroup);

                if let Some(group) = &self.current_group {
                    Frame::none().inner_margin(Margin::symmetric(10.0, 15.0)).show(ui, |ui| {
                        ui.label(format!("Invita in '{}':", group.name));
                            ui.text_edit_singleline(&mut self.invite_user_input);
                        if ui.button("✉ Invita Utente").clicked() {
                            let _ = self.to_backend_tx.try_send(ToBackend::InviteUser(
                                group.name.clone(),
                                self.invite_user_input.clone(),
                            ));
                            self.invite_user_input.clear();
                        }
                    });
                }
                self.draw_info_error_messages(ui);
            });
        });

    // --- SEZIONE MODIFICATA ---
    if let Some(group) = &self.current_group {
        // NUOVO: Pannello inferiore per l'input del messaggio
        egui::TopBottomPanel::bottom("chat_input_panel")
            .resizable(false)
            .min_height(40.0)
            .show(ctx, |ui| {
                ui.separator();
                // Usiamo un layout orizzontale per centrare il campo di testo e lasciare spazio ai lati
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let text_edit_response = ui.add_sized(
                        ui.available_size(), // Usa tutto lo spazio disponibile nel pannello
                        egui::TextEdit::singleline(&mut self.chat_message_input)
                        .hint_text(format!("Messaggio in #{}", group.name))
                        .frame(false)
                    );
                    
                    // Logica di invio del messaggio (spostata qui)
                    if text_edit_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.chat_message_input.is_empty() {
                        let _ = self.to_backend_tx.try_send(ToBackend::SendMessage(self.chat_message_input.clone()));
                        self.chat_message_input.clear();
                        text_edit_response.request_focus();
                    }
                });
            });

        // MODIFICATO: Il pannello centrale ora contiene solo i messaggi
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.heading(format!("# {}", group.name));
            });
            ui.separator();

            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false; 2]).show(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                    ui.add_space(10.0); // Aggiunge un po' di spazio in cima
                    for msg in &self.messages {
                        self.draw_message_bubble(ui, msg);
                    }
                });
            });
        });
    } else {
        // Questo non cambia, viene mostrato se non si è in nessun gruppo
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label("Crea o unisciti a un gruppo per iniziare.");
            });
        });
    }
}

    fn draw_group_section(ui: &mut egui::Ui, title: &str, input: &mut String, to_backend_tx: &Sender<ToBackend>, action: fn(String) -> ToBackend) {
        Frame::none().inner_margin(Margin::symmetric(10.0, 15.0)).show(ui, |ui| {
             ui.label(title);
             ui.text_edit_singleline(input);
             let button_text = if title.contains("Crea") { "➕ Crea" } else { "➡️ Join" };
             if ui.button(button_text).clicked() {
                let _ = to_backend_tx.try_send(action(input.clone()));
                input.clear();
            }
        });
    }
    
    fn draw_message_bubble(&self, ui: &mut egui::Ui, msg: &WsServerMessage) {
        let is_my_message = self.current_user.as_ref().unwrap().id == msg.sender_id;
        let layout = if is_my_message {
            Layout::right_to_left(Align::TOP)
        } else {
            Layout::left_to_right(Align::TOP)
        };
        
        ui.with_layout(layout, |ui| {
             Frame::none()
                .inner_margin(Margin::symmetric(12.0, 8.0))
                .rounding(Rounding { nw: 12.0, ne: 12.0, sw: if is_my_message { 2.0 } else { 12.0 }, se: if is_my_message { 12.0 } else { 2.0 }})
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

// --- Funzione per lo Stile ---
fn configure_styles(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let visuals = &mut style.visuals;

    // Palette Ispirata a Catppuccin Frappé
    let bg_main = egui::Color32::from_rgb(48, 52, 70);
    let bg_secondary = egui::Color32::from_rgb(69, 73, 94);
    let fg_text = egui::Color32::from_rgb(198, 208, 228);
    let accent_color = egui::Color32::from_rgb(136, 192, 208); // Teal
    let widget_stroke = Stroke::new(1.0, egui::Color32::from_rgb(88, 91, 112));

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

    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(110, 115, 141);
    visuals.widgets.hovered.rounding = Rounding::same(4.0);

    visuals.widgets.active.bg_fill = accent_color;
    visuals.widgets.active.rounding = Rounding::same(4.0);
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, egui::Color32::WHITE);
    
    visuals.selection.bg_fill = accent_color;
    visuals.selection.stroke = Stroke::new(1.0, fg_text);
    
    // Stile per i pulsanti
    style.spacing.button_padding = Vec2::new(10.0, 8.0);

    ctx.set_style(style);
}

// --- Logica di Rete Asincrona (MODIFICATA PER PASSARE L'ID UTENTE) ---
// (Il resto delle funzioni di rete rimane perlopiù invariato, ma join_group è stato aggiornato)

async fn handle_join_group(
    client: &HttpClient,
    group_name: String,
    user: User, // Ora riceviamo l'intero utente
    token: String,
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

    // MODIFICA PER LA SICUREZZA: Passa il token per l'autenticazione del WebSocket
    // Questo è un passo intermedio. L'ideale sarebbe che il server validasse il token.
    let ws_url = format!(
        "ws://127.0.0.1:3000/groups/{}/chat?user_id={}&token={}", // Aggiunto token
        group.id, user.id, token
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

    (Some(tx), FromBackend::GroupJoined(group, user.id))
}


// Il resto delle funzioni (handle_register, handle_login, etc.) e la funzione main rimangono invariate.
// ... (copia qui le restanti funzioni di rete dal tuo file originale) ...

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
    
    // Per ottenere l'utente, sarebbe meglio se l'endpoint di login restituisse già i dati dell'utente
    // insieme al token. Per ora, lo richiediamo separatamente.
    let mut headers = header::HeaderMap::new();
    headers.insert(header::AUTHORIZATION, header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
    let authed_client = HttpClient::builder().default_headers(headers).build().unwrap();

    let user_res = match authed_client
        .get(format!("{}/users/by_username/{}", API_BASE_URL, username))
        .send().await
    {
        Ok(res) if res.status().is_success() => res.json::<User>().await.map_err(|_| FromBackend::Error("Errore recupero dati utente.".into())),
        _ => return Err(FromBackend::Error("Impossibile recuperare i dati utente dopo il login.".into())),
    };

    Ok(FromBackend::LoggedIn(user_res.unwrap(), token))
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


// --- Funzione Main ---
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
