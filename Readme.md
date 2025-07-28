password PostgreSQL: ciao

DATABASES TABLES

users

Contiene le informazioni di base per ogni utente registrato.

id: (UUID) L'identificativo unico dell'utente.

username: (TEXT) Il nome utente, che deve essere unico.

created_at: (TIMESTAMPTZ) La data e l'ora di registrazione.


groups
Memorizza i gruppi di chat creati.

id: (UUID) L'identificativo unico del gruppo.

name: (TEXT) Il nome del gruppo.

created_at: (TIMESTAMPTZ) La data e l'ora di creazione.


group_members
Tabella di collegamento che associa gli utenti ai gruppi di cui fanno parte.

user_id: (UUID) Chiave esterna che fa riferimento a users.id.

group_id: (UUID) Chiave esterna che fa riferimento a groups.id.



group_invitations
Traccia gli inviti in sospeso per entrare nei gruppi.

id: (UUID) L'identificativo unico dell'invito.

group_id: (UUID) Il gruppo a cui l'utente è stato invitato.

inviter_id: (UUID) L'utente che ha inviato l'invito.

invited_user_id: (UUID) L'utente che ha ricevuto l'invito.

status: (VARCHAR) Lo stato dell'invito (es. 'pending', 'accepted').



messages
Archivia tutti i messaggi inviati all'interno dei gruppi.

id: (UUID) L'identificativo unico del messaggio.

group_id: (UUID) Il gruppo in cui è stato inviato il messaggio.

sender_id: (UUID) L'utente che ha inviato il messaggio.

content: (TEXT) Il contenuto testuale del messaggio.

created_at: (TIMESTAMPTZ) La data e l'ora di invio.



_sqlx_migrations
Tabella interna gestita da SQLx per tenere traccia delle migrazioni del database applicate.


API REGISTRAZIONE

http://127.0.0.1:3000/users/register


BODY RICHIESTA POST

 {
    "username": "primo_utente"
}

BODY RISPOSTA

{
    "id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
    "username": "primo_utente",
    "created_at": "2025-07-22T21:10:00.123Z"
}

API CREARE GRUPPO

http://127.0.0.1:3000/groups

BODY RICHIESTA 

{
    "name": "Gruppo di Prova",
    "creator_id": "ID_DEL_PRIMO_UTENTE_COPIATO_PRIMA" 
}

BODY RISPOSTA

{
    "id": "f9e8d7c6-b5a4-3210-fedc-ba9876543210",
    "name": "Gruppo di Prova",
    "created_at": "2025-07-22T21:12:00.456Z"
}

API INVITARE ALTRO UTENTE

http://127.0.0.1:3000/groups/ID_DEL_GRUPPO/invite 

BODY RICHIESTA

{
    "inviter_id": "ID_DEL_PRIMO_UTENTE",
    "user_to_invite_id": "ID_DEL_SECONDO_UTENTE"
}

RISPOSTA: 201 Created