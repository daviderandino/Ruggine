
# Ruggine - Documentazione API & DB
Questa è la documentazione del server di Ruggine.

## Autenticazione

La maggior parte delle API richiede un token JWT per l'autenticazione. Il token deve essere inviato nell'header Authorization con lo schema Bearer.

Authorization: Bearer <your_jwt_token>

## API Utenti
### 1. Registrazione Utente

Crea un nuovo utente nel sistema.

- Endpoint: POST http://127.0.0.1:3000/users/register

- Descrizione: Registra un nuovo utente con username e password. La password deve essere lunga almeno 8 caratteri.

Corpo della Richiesta:

<pre>
{
    "username": "mio_utente",
    "password": "password_sicura_123"
}
</pre>

Risposta di Successo (Codice 200 OK):

<pre>
{
    "id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
    "username": "mio_utente",
    "created_at": "2025-07-29T20:30:00.123Z"
}
</pre>

Risposte di Errore:

- 400 Bad Request: La password è troppo corta.

- 409 Conflict: L'username esiste già.

### 2. Login Utente
Autentica un utente e restituisce un token JWT.

- Endpoint: POST http://127.0.0.1:3000/users/login

- Descrizione: Effettua il login fornendo username e password per ottenere un token di sessione.

Corpo della Richiesta (application/json):

<pre>
{
    "username": "mio_utente",
    "password": "password_sicura_123"
}
</pre>

Risposta di Successo (Codice 200 OK):

<pre>
{
    {
    "token": "ey...un_lungo_token_jwt...A",
    "user": {
        "id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
        "username": "mio_utente",
        "created_at": "2025-07-30T22:15:00.123Z"
    },
    "groups": [
        {
            "id": "f9e8d7c6-b5a4-3210-fedc-ba9876543210",
            "name": "Il Mio Gruppo Fantastico",
            "created_at": "2025-07-30T22:20:00.456Z"
        }
    ]
}
}
</pre>

Risposte di Errore:

- 401 Unauthorized: Username o password non validi.

### 3. Ottenere Utente per Username
Recupera le informazioni di un utente specifico.

- Endpoint: GET http://127.0.0.1:3000/users/by_username/<username>

- Descrizione: Cerca e restituisce i dati di un utente dato il suo username.

Parametri URL:

- username: Il nome dell'utente da cercare.

Risposta di Successo (Codice 200 OK):

<pre>
{
    "id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
    "username": "mio_utente",
    "created_at": "2025-07-29T20:30:00.123Z"
}
</pre>
Risposte di Errore:

- 404 Not Found: Utente non trovato.

## API Gruppi
### 1. Creare un Gruppo
Crea un nuovo gruppo di chat.

- Endpoint: POST http://127.0.0.1:3000/groups

- Descrizione: Crea un nuovo gruppo e aggiunge l'utente creatore come primo membro.

Corpo della Richiesta (application/json):

<pre>
{
    "name": "Il Mio Gruppo Fantastico",
}
</pre>
Risposta di Successo (Codice 200 OK):

<pre>
{
    "id": "f9e8d7c6-b5a4-3210-fedc-ba9876543210",
    "name": "Il Mio Gruppo Fantastico",
    "created_at": "2025-07-29T20:35:00.456Z"
}
</pre>

### 2. Ottenere Gruppo per Nome
Recupera le informazioni di un gruppo specifico.

- Endpoint: GET http://127.0.0.1:3000/groups/by_name/<name>

- Descrizione: Cerca e restituisce i dati di un gruppo dato il suo nome.

Parametri URL:

- name: Il nome del gruppo da cercare.

Risposta di Successo (Codice 200 OK):

<pre>
{
    "id": "f9e8d7c6-b5a4-3210-fedc-ba9876543210",
    "name": "Il Mio Gruppo Fantastico",
    "created_at": "2025-07-29T20:35:00.456Z"
}
</pre>
Risposte di Errore:

- 404 Not Found: Gruppo non trovato.

### 3. Invitare un Utente in un Gruppo
Invia un invito a un utente per unirsi a un gruppo.

- Endpoint: POST http://127.0.0.1:3000/groups/<group_id>/invite

- Descrizione: Permette a un membro di un gruppo di invitare un altro utente.

Parametri URL:

- group_id: L'ID del gruppo in cui invitare.

Corpo della Richiesta (application/json):

<pre>
{
    "user_to_invite_id": "b2c3d4e5-f6a7-8901-2345-67890abcdef1"
}
</pre>

Risposta di Successo: Codice 201 Created.

Risposte di Errore:

- 403 Forbidden: L'utente che invita non è membro del gruppo.

- 404 Not Found: L'utente o il gruppo specificato non esiste.

- 409 Conflict: L'utente è già membro o un invito è già stato inviato.

### 4. Ottenere Messaggi di un Gruppo
Recupera la cronologia dei messaggi di un gruppo.

- Endpoint: GET http://127.0.0.1:3000/groups/<group_id>/messages

- Descrizione: Restituisce gli ultimi 100 messaggi di un gruppo. Richiede che l'utente sia membro del gruppo.

Parametri URL:

- group_id: L'ID del gruppo di cui recuperare i messaggi.

Risposta di Successo (Codice 200 OK):
<pre>
[
    {
        "sender_id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
        "sender_username": "mio_utente",
        "content": "Ciao a tutti!"
    },
    {
        "sender_id": "b2c3d4e5-f6a7-8901-2345-67890abcdef1",
        "sender_username": "altro_utente",
        "content": "Ciao!"
    }
]
</pre>
Risposte di Errore:

- 403 Forbidden: L'utente non è membro del gruppo.

### 5. Uscire da un Gruppo
Permette a un utente di lasciare un gruppo.

- Endpoint: DELETE http://127.0.0.1:3000/groups/<group_id>/leave

- Descrizione: Rimuove l'utente (identificato dal token JWT) dai membri del gruppo. Se l'utente è l'ultimo membro, il gruppo viene eliminato.

Parametri URL:

- group_id: L'ID del gruppo da cui uscire.

Risposta di Successo: Codice 204 No Content.

## 2. API Inviti

### 1. Ottenere Inviti Pendenti
Recupera la lista di tutti gli inviti pendenti per l'utente loggato.

- Endpoint: GET http://127.0.0.1:3000/invitations

- Descrizione: Restituisce una lista di inviti che l'utente (identificato dal token JWT) non ha ancora accettato o rifiutato.

Risposta di Successo (Codice 200 OK):
<pre>
[
    {
        "id": "c3d4e5f6-a7b8-9012-3456-7890abcdef12",
        "group_id": "f9e8d7c6-b5a4-3210-fedc-ba9876543210",
        "group_name": "Il Mio Gruppo Fantastico",
        "inviter_username": "mio_utente"
    }
]
</pre>
### 2. Accettare un Invito
Accetta un invito per entrare in un gruppo.

- Endpoint: POST http://127.0.0.1:3000/invitations/<invitation_id>/accept

- Descrizione: Permette all'utente (identificato dal token JWT) di accettare un invito, diventando membro del gruppo.

Parametri URL:

- invitation_id: L'ID dell'invito da accettare.

Risposta di Successo (Codice 200 OK): I dati del gruppo a cui si è unito.

Risposte di Errore:

- 404 Not Found: Invito non trovato o non destinato all'utente.

### 3. Rifiutare un Invito
Rifiuta un invito per entrare in un gruppo.

- Endpoint: POST http://127.0.0.1:3000/invitations/<invitation_id>/decline

- Descrizione: Permette all'utente (identificato dal token JWT) di rifiutare un invito.

Parametri URL:

- invitation_id: L'ID dell'invito da rifiutare.

Risposta di Successo: Codice 204 No Content.

Risposte di Errore:

- 404 Not Found: Invito non trovato o non destinato all'utente.

## WebSocket Chat
### 1. Connessione alla Chat
Stabilisce una connessione WebSocket per ricevere e inviare messaggi in tempo reale.

- Endpoint: GET /groups/:group_id/chat

- Descrizione: Esegue l'upgrade della connessione da HTTP a WebSocket.

Parametri Query String:

- group_id: L'ID del gruppo a cui connettersi.

- user_id: L'ID dell'utente che si sta connettendo.

- token: Il token JWT per l'autenticazione.

Esempio URL:
ws://127.0.0.1:3000/groups/f9e8d7c6-b5a4-3210-fedc-ba9876543210/chat?user_id=a1b2c3d4-e5f6-7890-1234-567890abcdef&token=ey...

### 2. Messaggi WebSocket
Messaggio dal Client al Server:
Il client invia un oggetto JSON con il contenuto del messaggio.

<pre>
{
    "content": "Ciao a tutti!"
}
</pre>

Messaggio dal Server al Client:
Il server inoltra i messaggi a tutti i client connessi, aggiungendo le informazioni sul mittente.

<pre>
{
    "sender_id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
    "sender_username": "mio_utente",
    "content": "Ciao a tutti!"
}
</pre>

#  DATABASE

## 🗄️ Tabella `users`
Contiene le informazioni di base per ogni utente registrato.

- **id (UUID)**: L'identificativo unico dell'utente (Chiave Primaria).
- **username (TEXT)**: Il nome utente, che deve essere unico nel sistema.
- **password_hash (TEXT)**: L'hash della password dell'utente. Non viene mai esposto al client.
- **created_at (TIMESTAMPTZ)**: La data e l'ora di registrazione dell'utente.

---
## 🗄️ Tabella `groups`
Memorizza i gruppi di chat creati.

- **id (UUID)**: L'identificativo unico del gruppo (Chiave Primaria).
- **name (TEXT)**: Il nome del gruppo, che può non essere unico.
- **created_at (TIMESTAMPTZ)**: La data e l'ora di creazione del gruppo.

---
## 🗄️ Tabella `group_members`
Tabella di collegamento (o "ponte") che associa gli utenti ai gruppi di cui fanno parte.

- **user_id (UUID)**: Chiave esterna che fa riferimento a `users(id)`.
- **group_id (UUID)**: Chiave esterna che fa riferimento a `groups(id)`.
- La coppia `(user_id, group_id)` forma la **chiave primaria composita**, garantendo che un utente possa essere membro di un gruppo una sola volta.

---
## 🗄️ Tabella `group_invitations`
Traccia gli inviti in sospeso per entrare nei gruppi.

- **id (UUID)**: L'identificativo unico dell'invito (Chiave Primaria).
- **group_id (UUID)**: Chiave esterna che fa riferimento al gruppo a cui l'utente è stato invitato.
- **inviter_id (UUID)**: Chiave esterna che fa riferimento all'utente che ha inviato l'invito.
- **invited_user_id (UUID)**: Chiave esterna che fa riferimento all'utente che ha ricevuto l'invito.
- **status (VARCHAR)**: Lo stato dell'invito. I valori possibili sono: `'pending'`, `'accepted'`, `'declined'`.
- La coppia `(group_id, invited_user_id)` dovrebbe avere un vincolo di **unicità** per evitare inviti duplicati.

---
## 🗄️ Tabella `group_messages`
Archivia tutti i messaggi inviati all'interno dei gruppi per creare la cronologia.

- **id (UUID)**: L'identificativo unico del messaggio (Chiave Primaria).
- **group_id (UUID)**: Chiave esterna che fa riferimento al gruppo in cui è stato inviato il messaggio.
- **user_id (UUID)**: Chiave esterna che fa riferimento all'utente che ha inviato il messaggio.
- **content (TEXT)**: Il contenuto testuale del messaggio.
- **created_at (TIMESTAMPTZ)**: La data e l'ora di invio del messaggio.

---
## 🗄️ Tabella `_sqlx_migrations`
Tabella interna gestita automaticamente da `sqlx-cli` per tenere traccia delle migrazioni del database che sono state applicate. Non interagire direttamente con questa tabella.
