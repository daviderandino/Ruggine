
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
    "token": "ey...un_lungo_token_jwt...A"
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
    "creator_id": "a1b2c3d4-e5f6-7890-1234-567890abcdef"
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
    "inviter_id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
    "user_to_invite_id": "b2c3d4e5-f6a7-8901-2345-67890abcdef1"
}
</pre>

Risposta di Successo: Codice 201 Created.

Risposte di Errore:

- 403 Forbidden: L'utente che invita non è membro del gruppo.

- 404 Not Found: L'utente o il gruppo specificato non esiste.

- 409 Conflict: L'utente è già membro o un invito è già stato inviato.

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

# DATABASE
## 🗄️ Tabella users
Contiene le informazioni di base per ogni utente registrato.

- id (UUID): L'identificativo unico dell'utente.

- username (TEXT): Il nome utente, che deve essere unico.

- password_hash (TEXT): L'hash della password dell'utente, non viene mai inviato al client.

- created_at (TIMESTAMPTZ): La data e l'ora di registrazione.

## 🗄️ Tabella groups
Memorizza i gruppi di chat creati.

- id (UUID): L'identificativo unico del gruppo.

- name (TEXT): Il nome del gruppo.

- created_at (TIMESTAMPTZ): La data e l'ora di creazione.

## 🗄️ Tabella group_members
Tabella di collegamento che associa gli utenti ai gruppi di cui fanno parte.

- user_id (UUID): Chiave esterna che fa riferimento a users.id.

- group_id (UUID): Chiave esterna che fa riferimento a groups.id.

## 🗄️ Tabella group_invitations
Traccia gli inviti in sospeso per entrare nei gruppi.

- id (UUID): L'identificativo unico dell'invito.

- group_id (UUID): Il gruppo a cui l'utente è stato invitato.

- inviter_id (UUID): L'utente che ha inviato l'invito.

- invited_user_id (UUID): L'utente che ha ricevuto l'invito.

- status (VARCHAR): Lo stato dell'invito (es. 'pending', 'accepted').

## 🗄️ Tabella messages
Archivia tutti i messaggi inviati all'interno dei gruppi.

- id (UUID): L'identificativo unico del messaggio.

- group_id (UUID): Il gruppo in cui è stato inviato il messaggio.

- sender_id (UUID): L'utente che ha inviato il messaggio.

- content (TEXT): Il contenuto testuale del messaggio.

- created_at (TIMESTAMPTZ): La data e l'ora di invio.

## 🗄️ Tabella _sqlx_migrations
Tabella interna gestita da SQLx per tenere traccia delle migrazioni del database applicate.