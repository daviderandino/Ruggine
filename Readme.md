password PostgreSQL: ciao


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