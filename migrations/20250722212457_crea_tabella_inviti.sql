-- Add migration script here
-- File: migrations/<timestamp>_crea_tabella_inviti.sql

-- Tabella per tracciare gli inviti ai gruppi
CREATE TABLE group_invitations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    inviter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invited_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'pending', -- es. 'pending', 'accepted', 'declined'
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Un utente può avere un solo invito in sospeso per un dato gruppo
    UNIQUE(group_id, invited_user_id)
);