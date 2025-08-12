use super::*;
pub fn configure_styles(ctx: &egui::Context) {
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

impl RuggineApp{
    pub fn draw_auth_view(&mut self, ctx: &egui::Context) {
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

    pub fn draw_main_view(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("side_panel").min_width(250.0).default_width(250.0).show(ctx, |ui| {
            ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.heading(format!("Ciao, {}!", self.current_user.as_ref().unwrap().username));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("🚪 Logout").on_hover_text("Esci dall'account").clicked() {
                            self.current_user = None;
                            self.auth_token = None;
                            self.user_groups.clear();
                            self.selected_group_id = None;
                            self.messages.clear();
                            self.pending_invitations.clear();
                        }
                    });
                });
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
                if self.selected_group_id.is_some(){
                // Lista dei gruppi a cui l'utente appartiene
                ui.heading("I Miei Gruppi");
                // Modifica qui: usa `ui.push_id` per creare un contesto con ID univoco per lo ScrollArea
                ui.push_id("my_groups_scroll_area", |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for group in self.user_groups.clone() {
                            let is_selected = self.selected_group_id == Some(group.id);
                            if ui.selectable_value(&mut self.selected_group_id, Some(group.id), format!("# {}", group.name)).clicked() {
                                self.to_backend_tx.try_send(ToBackend::FetchGroupMessages(group.id)).ok();
                                self.to_backend_tx.try_send(ToBackend::FetchGroupMembers(group.id)).ok();
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
            }
                // Sezione: Lista membri del gruppo selezionato
                if let Some(selected_id) = self.selected_group_id {
                    if let Some(group) = self.user_groups.iter().find(|g| g.id == selected_id) {
                        ui.heading(format!("Membri di '{}'", group.name));
                        // Fetch members from backend (if not already fetched)
                        // For simplicity, we fetch every time the group changes
                        // You may want to cache this in a real app
                        if let Some(members) = self.selected_group_members.clone(){
                        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                            for member in members {
                                ui.label(format!("• {}", member.username));
                            }                            
                        });
                    }
                }
                else{
                    ui.label("Nessun membro trovato."); //Dovrebbe panicare per come è fatta la UI, 
                }
                }
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

    pub fn draw_invitations_section(&mut self, ui: &mut egui::Ui) {
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
                            ui.label(egui::RichText::new(&invitation.group_name).strong().color(egui::Color32::WHITE));
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

    pub fn draw_message_bubble(&self, ui: &mut egui::Ui, msg: &WsServerMessage) {
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

    pub fn draw_info_error_messages(&self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        if let Some(info) = &self.info_message {
            ui.label(egui::RichText::new(info).color(egui::Color32::from_rgb(166, 209, 137)));
        }
        if let Some(err) = &self.error_message {
            ui.label(egui::RichText::new(err).color(egui::Color32::from_rgb(237, 135, 150)));
        }
    }
}
