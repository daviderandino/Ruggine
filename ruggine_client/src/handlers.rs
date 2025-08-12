use tokio_tungstenite::tungstenite::client;

use super::*;

// --- Network Logic ---
pub async fn handle_register(client: &HttpClient, username: String, password: String) -> FromBackend {
    if username.is_empty() || password.is_empty() { return FromBackend::Error("Username e password non possono essere vuoti.".into()); }
    let payload = serde_json::json!({ "username": username, "password": password });
    match client.post(format!("{}/users/register", API_BASE_URL)).json(&payload).send().await {
        Ok(res) if res.status().is_success() => FromBackend::Registered,
        Ok(res) => {
            if res.status() == StatusCode::BAD_REQUEST {
                return FromBackend::Error("La password è troppo corta.".into());
            }
            else if res.status() == StatusCode::CONFLICT {
                return FromBackend::Error("Nome utente già in uso.".into());
            }
            else {
                return FromBackend::Error(
                    res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into()),
                );
            }
        }
        Err(_) => FromBackend::Error("Impossibile connettersi al server.".into()),
    }
}

pub async fn handle_login(
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
        Ok(res) => {
            if res.status() == StatusCode::UNAUTHORIZED {
                return Err(FromBackend::Error("Username e password errati.".into()));
            }
            else if res.status() == StatusCode::NOT_FOUND {
                return Err(FromBackend::Error("Utente non trovato.".into()));
            }
            else {
                return Err(FromBackend::Error(
                    res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into()),
                ));
            }
        }
        Err(_) => Err(FromBackend::Error(
            "Impossibile connettersi al server.".into(),
        )),
    }
}

pub async fn handle_create_group(client: &HttpClient, name: String) -> Result<Group, FromBackend> {
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

pub async fn handle_leave_group(client: &HttpClient, group_id: Uuid) -> FromBackend {
    match client.delete(format!("{}/groups/{}/leave", API_BASE_URL, group_id)).send().await {
        Ok(res) if res.status().is_success() => FromBackend::GroupLeft(group_id),
        Ok(res) => FromBackend::Error(res.text().await.unwrap_or_else(|_| "Errore durante l'uscita dal gruppo.".into())),
        Err(_) => FromBackend::Error("Errore di connessione.".into()),
    }
}

pub async fn handle_invite(
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
            if res.status() == StatusCode::FORBIDDEN {
                return FromBackend::Error("Errore, l'utente che invita non è membro del gruppo.".into());
            }
            else if res.status() == StatusCode::NOT_FOUND {
                return FromBackend::Error("L'utente o il gruppo non esistono.".into());
            }
            else if res.status() == StatusCode::CONFLICT {
                return FromBackend::Error("L'utente è già membro del gruppo.".into());
            }
            else {
                return FromBackend::Error(
                    res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into()),
                );
            }
        ),
        Err(_) => FromBackend::Error("Errore di connessione durante l'invito.".into()),
    }
}

pub async fn handle_fetch_invitations(client: &HttpClient) -> FromBackend {
    match client.get(format!("{}/invitations", API_BASE_URL)).send().await {
        Ok(res) if res.status().is_success() => match res.json::<Vec<Invitation>>().await {
            Ok(invitations) => FromBackend::InvitationsFetched(invitations),
            Err(_) => FromBackend::Error("Errore nel decodificare gli inviti.".into()),
        },
        _ => FromBackend::Error("Impossibile recuperare gli inviti.".into()),
    }
}

pub async fn handle_accept_invitation(client: &HttpClient, id: Uuid) -> Result<Group, FromBackend> {
    match client.post(format!("{}/invitations/{}/accept", API_BASE_URL, id)).send().await {
        Ok(res) if res.status().is_success() => res.json::<Group>().await.map_err(|_| FromBackend::Error("Errore decodifica gruppo.".into())),
        Ok(res) => {
            if res.status() == StatusCode::NOT_FOUND {
                return Err(FromBackend::Error("Inviti non trovati.".into()));   
            }
            else {
                return Err(FromBackend::Error(
                    res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into()),
                ));
            }
        }
        Err(_) => Err(FromBackend::Error("Errore di connessione.".into())),
    }
}

pub async fn handle_decline_invitation(client: &HttpClient, id: Uuid) -> FromBackend {
    match client.post(format!("{}/invitations/{}/decline", API_BASE_URL, id)).send().await {
        Ok(res) if res.status().is_success() => FromBackend::InvitationDeclined(id),
        Ok(res) => {
            if res.status() == StatusCode::NOT_FOUND {
                return FromBackend::Error("Inviti non trovati.".into());   
            }
            else {
                return FromBackend::Error(
                    res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into()),
                );
            }
        }
        Err(_) => FromBackend::Error("Errore di connessione.".into()),
    }
}

pub async fn handle_fetch_group_messages(client: &HttpClient, group_id: Uuid) -> FromBackend {
    match client.get(format!("{}/groups/{}/messages", API_BASE_URL, group_id)).send().await {
        Ok(res) if res.status().is_success() => {
            match res.json::<Vec<WsServerMessage>>().await {
                Ok(messages) => FromBackend::GroupMessagesFetched(group_id, messages),
                Err(_) => FromBackend::Error("Errore nel decodificare la cronologia dei messaggi.".into()),
            }
        },
        Ok(res) => {
            if res.status() == StatusCode::FORBIDDEN {
                return FromBackend::Error("Accesso negato, l'utente non è membro del gruppo.".into());
            }
            else {
                return FromBackend::Error(
                    res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".into()),
                );
            }
        }
        Err(_) => FromBackend::Error("Errore di connessione per la cronologia dei messaggi.".into()),
    }
}

pub async fn handle_fetch_group_members(client: &HttpClient, group_id: Uuid) -> FromBackend {
    match client.get(format!("{}/groups/{}/members", API_BASE_URL, group_id)).send().await {
        Ok(res) if res.status().is_success() => {
            match res.json::<Vec<User>>().await {
                Ok(membri) => FromBackend::GroupMembersFetched(group_id, membri),
                Err(_) => FromBackend::Error("Errore nel decodificare i membri del gruppo.".to_string()),
            }
        }
        Ok(res) => {
            FromBackend::Error(res.text().await.unwrap_or_else(|_| "Errore sconosciuto.".to_string()))
        }
        Err(_) => FromBackend::Error("Errore richiesta handle fetch".to_string()),
    }
}

pub async fn handle_join_group(
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

