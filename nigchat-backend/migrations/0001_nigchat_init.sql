-- ===========================================================================
-- NigChat — complete initial schema
--
-- Single-file migration, per requirement. Everything the platform needs is
-- here: identity, devices, E2EE key distribution, conversations, messages,
-- groups, communities, channels, status, calls, notifications (including
-- per-conversation tones and quiet hours), media, moderation, backups,
-- security audit and feature flags.
--
-- Rules encoded in this file (spec §25, Appendix 3):
--   * PostgreSQL is the transactional source of truth.
--   * No media blobs live here — only object-storage keys and metadata.
--   * No plaintext secrets: OTPs, refresh tokens, PINs and passkey material
--     are stored hashed or as public material only.
--   * Message ciphertext is opaque to the server. The server routes and
--     orders; it does not read (spec §28).
--   * Ordering is a per-conversation monotonic `seq`, never a timestamp.
-- ===========================================================================

CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ---------------------------------------------------------------------------
-- Enumerated domains
--
-- CHECK constraints rather than PG enums: adding a value to an enum type is a
-- lock-taking DDL migration, while widening a CHECK is cheap. At this scale
-- that flexibility is worth more than the type strictness.
-- ---------------------------------------------------------------------------

-- ===========================================================================
-- 1. IDENTITY
-- ===========================================================================

CREATE TABLE users (
    id                  UUID PRIMARY KEY,
    phone_e164          TEXT        NOT NULL UNIQUE,
    -- Peppered HMAC of the number. Contact discovery matches on this so the
    -- server never receives the raw numbers of people who are not users.
    phone_hash          TEXT        UNIQUE,
    phone_country       TEXT,                      -- ISO 3166-1 alpha-2, for routing/analytics
    username            CITEXT      UNIQUE,
    -- Optional "username key" (spec §3): an extra secret that gates who may
    -- initiate contact by handle. Hash only — the server never needs the value.
    username_key_hash   TEXT,
    display_name        TEXT        NOT NULL,
    about               TEXT,
    avatar_media_id     UUID,                      -- FK added after media_assets
    -- Two-step verification PIN (spec §14). Argon2id, never reversible.
    two_step_pin_hash   TEXT,
    two_step_email      TEXT,                      -- recovery hint only
    two_step_enabled_at TIMESTAMPTZ,
    is_active           BOOLEAN     NOT NULL DEFAULT TRUE,
    deactivated_at      TIMESTAMPTZ,
    -- Set when an account is scheduled for deletion; a worker purges after the
    -- grace period so an accidental or coerced deletion is recoverable.
    delete_after        TIMESTAMPTZ,
    last_seen_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX users_phone_hash_idx  ON users (phone_hash) WHERE phone_hash IS NOT NULL;
CREATE INDEX users_username_trgm_idx ON users USING gin (username gin_trgm_ops);
CREATE INDEX users_delete_after_idx  ON users (delete_after) WHERE delete_after IS NOT NULL;

-- Privacy controls (spec §14). Split from `users` because it is read on a
-- different path (visibility checks) than the profile itself.
CREATE TABLE user_privacy_settings (
    user_id                 UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_seen_visibility    TEXT NOT NULL DEFAULT 'contacts'
                              CHECK (last_seen_visibility IN ('everyone','contacts','nobody')),
    profile_photo_visibility TEXT NOT NULL DEFAULT 'contacts'
                              CHECK (profile_photo_visibility IN ('everyone','contacts','nobody')),
    about_visibility        TEXT NOT NULL DEFAULT 'contacts'
                              CHECK (about_visibility IN ('everyone','contacts','nobody')),
    status_visibility       TEXT NOT NULL DEFAULT 'contacts'
                              CHECK (status_visibility IN ('everyone','contacts','nobody')),
    read_receipts_enabled   BOOLEAN NOT NULL DEFAULT TRUE,
    typing_indicators_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    who_can_add_to_groups   TEXT NOT NULL DEFAULT 'contacts'
                              CHECK (who_can_add_to_groups IN ('everyone','contacts','nobody')),
    who_can_call            TEXT NOT NULL DEFAULT 'everyone'
                              CHECK (who_can_call IN ('everyone','contacts','nobody')),
    silence_unknown_callers BOOLEAN NOT NULL DEFAULT FALSE,
    strict_privacy_mode     BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- Devices and sessions
-- ---------------------------------------------------------------------------

-- A linked device is a first-class principal: it holds its own E2EE identity
-- key and can be revoked independently. The primary phone is NOT a relay
-- (Appendix 3) — every device syncs from the server directly.
CREATE TABLE devices (
    id                UUID PRIMARY KEY,
    user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform          TEXT NOT NULL CHECK (platform IN
                        ('ios','ipados','android','android_tablet','web','windows','macos','linux')),
    device_name       TEXT,
    app_version       TEXT,
    os_version        TEXT,
    is_primary        BOOLEAN NOT NULL DEFAULT FALSE,
    -- Registration id from the E2EE protocol; pairs a device with its keys.
    registration_id   INTEGER,
    linked_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_active_at    TIMESTAMPTZ,
    last_ip_hash      TEXT,        -- hashed: anomaly detection without storing IPs
    revoked_at        TIMESTAMPTZ,
    revoked_reason    TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX devices_user_active_idx ON devices (user_id) WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX devices_one_primary_idx ON devices (user_id)
    WHERE is_primary AND revoked_at IS NULL;

-- Device-linking handshake (spec §11): the phone shows a QR, the new device
-- claims the code. Short TTL, single use.
CREATE TABLE device_link_requests (
    id              UUID PRIMARY KEY,
    code_hash       TEXT NOT NULL UNIQUE,
    user_id         UUID REFERENCES users(id) ON DELETE CASCADE,
    platform        TEXT NOT NULL,
    device_name     TEXT,
    claimed_by      UUID REFERENCES devices(id) ON DELETE SET NULL,
    approved_at     TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX device_link_requests_expiry_idx ON device_link_requests (expires_at);

-- Rotating, device-bound refresh tokens. `replaced_by` makes the rotation
-- chain auditable so reuse of a spent token is detectable as theft.
CREATE TABLE device_sessions (
    id           UUID PRIMARY KEY,
    user_id      UUID NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL UNIQUE,
    user_agent   TEXT,
    ip_hash      TEXT,
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ,
    replaced_by  UUID REFERENCES device_sessions(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX device_sessions_device_idx ON device_sessions (device_id) WHERE revoked_at IS NULL;

-- ---------------------------------------------------------------------------
-- Authentication material
-- ---------------------------------------------------------------------------

CREATE TABLE auth_challenges (
    id           UUID PRIMARY KEY,
    phone_e164   TEXT NOT NULL,
    channel      TEXT NOT NULL DEFAULT 'sms' CHECK (channel IN ('sms','voice','flash_call','email')),
    code_hash    TEXT NOT NULL,
    attempts     INT  NOT NULL DEFAULT 0,
    ip_hash      TEXT,
    expires_at   TIMESTAMPTZ NOT NULL,
    consumed_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX auth_challenges_phone_idx ON auth_challenges (phone_e164, created_at DESC);

-- Passkeys / WebAuthn (spec §3, §14). Only public material is stored, so a
-- database compromise yields nothing usable for authentication.
CREATE TABLE passkey_credentials (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id   BYTEA NOT NULL UNIQUE,
    public_key      BYTEA NOT NULL,
    sign_count      BIGINT NOT NULL DEFAULT 0,
    transports      TEXT[],
    aaguid          UUID,
    friendly_name   TEXT,
    last_used_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX passkey_credentials_user_idx ON passkey_credentials (user_id);

-- ===========================================================================
-- 2. END-TO-END ENCRYPTION KEY DISTRIBUTION (spec §28)
--
-- The server is a key *directory*, never a key holder. It stores public
-- identity keys, signed prekeys and one-time prekeys, hands them out to
-- establish sessions, and knows nothing that would let it read a message.
-- ===========================================================================

CREATE TABLE device_identity_keys (
    device_id           UUID PRIMARY KEY REFERENCES devices(id) ON DELETE CASCADE,
    user_id             UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    identity_public_key BYTEA NOT NULL,
    -- Bumped when a device re-registers keys. Peers compare this to decide
    -- whether to raise a "security code changed" warning (spec §14).
    key_version         INT   NOT NULL DEFAULT 1,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    rotated_at          TIMESTAMPTZ
);
CREATE INDEX device_identity_keys_user_idx ON device_identity_keys (user_id);

CREATE TABLE device_signed_prekeys (
    device_id   UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    key_id      INT  NOT NULL,
    public_key  BYTEA NOT NULL,
    signature   BYTEA NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, key_id)
);

-- Consumed on handout: each one-time prekey is used for exactly one session.
-- Deleting on fetch is the whole point, so a low count must trigger a
-- top-up push to the owning device.
CREATE TABLE device_one_time_prekeys (
    device_id   UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    key_id      INT  NOT NULL,
    public_key  BYTEA NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, key_id)
);
CREATE INDEX device_one_time_prekeys_device_idx ON device_one_time_prekeys (device_id);

-- ===========================================================================
-- 3. CONTACTS, BLOCKS
-- ===========================================================================

-- Phone numbers of non-users are stored hashed: contact sync must not build a
-- server-side social graph of people who never joined (spec §5, §14).
CREATE TABLE contacts (
    owner_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    phone_hash      TEXT NOT NULL,
    contact_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    display_name    TEXT,
    is_favourite    BOOLEAN NOT NULL DEFAULT FALSE,
    synced_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_id, phone_hash)
);
CREATE INDEX contacts_resolved_idx ON contacts (owner_id, contact_user_id)
    WHERE contact_user_id IS NOT NULL;

CREATE TABLE blocks (
    blocker_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason      TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CONSTRAINT no_self_block CHECK (blocker_id <> blocked_id)
);
CREATE INDEX blocks_blocked_idx ON blocks (blocked_id);

-- ===========================================================================
-- 4. CONVERSATIONS
--
-- One table for direct chats, groups and channels. A single messaging
-- pipeline then serves all three, so any feature built for one works for all.
-- ===========================================================================

CREATE TABLE communities (
    id            UUID PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    avatar_media_id UUID,
    created_by    UUID REFERENCES users(id) ON DELETE SET NULL,
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE community_members (
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id)       ON DELETE CASCADE,
    role         TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner','admin','member')),
    joined_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (community_id, user_id)
);

CREATE TABLE conversations (
    id              UUID PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('direct','group','channel')),
    -- Sorted user-id pair; the unique index makes "open a chat with X"
    -- idempotent and race-free across instances.
    direct_key      TEXT UNIQUE,
    community_id    UUID REFERENCES communities(id) ON DELETE CASCADE,
    title           TEXT,
    description     TEXT,
    avatar_media_id UUID,
    created_by      UUID REFERENCES users(id) ON DELETE SET NULL,
    -- Group settings (spec §6)
    only_admins_can_post    BOOLEAN NOT NULL DEFAULT FALSE,
    only_admins_can_edit    BOOLEAN NOT NULL DEFAULT TRUE,
    approve_new_members     BOOLEAN NOT NULL DEFAULT FALSE,
    disappearing_seconds    INT,     -- NULL = off
    max_members             INT NOT NULL DEFAULT 1024,
    is_archived_globally    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT direct_needs_key      CHECK (kind <> 'direct'  OR direct_key IS NOT NULL),
    CONSTRAINT channel_needs_community CHECK (kind <> 'channel' OR community_id IS NOT NULL)
);
CREATE INDEX conversations_community_idx ON conversations (community_id) WHERE community_id IS NOT NULL;

-- Read state is a high-water mark, not a row per message per member.
-- A 500-member group with 1M messages would otherwise need 500M receipt rows.
CREATE TABLE conversation_members (
    conversation_id     UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id             UUID NOT NULL REFERENCES users(id)         ON DELETE CASCADE,
    role                TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner','admin','member')),
    last_read_seq       BIGINT NOT NULL DEFAULT 0,
    last_delivered_seq  BIGINT NOT NULL DEFAULT 0,
    -- Per-member view state (spec §17)
    is_pinned           BOOLEAN NOT NULL DEFAULT FALSE,
    is_archived         BOOLEAN NOT NULL DEFAULT FALSE,
    is_locked           BOOLEAN NOT NULL DEFAULT FALSE,  -- chat lock (spec §14)
    joined_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    invited_by          UUID REFERENCES users(id) ON DELETE SET NULL,
    left_at             TIMESTAMPTZ,
    PRIMARY KEY (conversation_id, user_id)
);
CREATE INDEX conversation_members_user_idx ON conversation_members (user_id) WHERE left_at IS NULL;

-- Monotonic per-conversation sequence. Allocated inside the send transaction;
-- the row lock serialises sends to ONE conversation and nothing else.
CREATE TABLE conversation_counters (
    conversation_id UUID PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    last_seq        BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE group_invites (
    id              UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    code            TEXT NOT NULL UNIQUE,
    created_by      UUID REFERENCES users(id) ON DELETE SET NULL,
    max_uses        INT,
    use_count       INT NOT NULL DEFAULT 0,
    expires_at      TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Append-only membership/settings history. Renders "X added Y" in the timeline
-- and doubles as the audit trail for group disputes.
CREATE TABLE group_events (
    id              UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    actor_id        UUID REFERENCES users(id) ON DELETE SET NULL,
    target_id       UUID REFERENCES users(id) ON DELETE SET NULL,
    event_type      TEXT NOT NULL CHECK (event_type IN
                      ('created','member_added','member_removed','member_left','role_changed',
                       'title_changed','avatar_changed','settings_changed','invite_revoked')),
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX group_events_conversation_idx ON group_events (conversation_id, created_at DESC);

-- ===========================================================================
-- 5. MESSAGES
--
-- `ciphertext` is opaque. The server orders and routes; it cannot read
-- (spec §28). `body_plaintext` exists ONLY for server-authored system
-- messages, which have no sender and no privacy expectation.
-- ===========================================================================

CREATE TABLE messages (
    id                  UUID   PRIMARY KEY,
    conversation_id     UUID   NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    seq                 BIGINT NOT NULL,
    sender_id           UUID   REFERENCES users(id)   ON DELETE SET NULL,
    sender_device_id    UUID   REFERENCES devices(id) ON DELETE SET NULL,
    -- Client-generated. Retrying a send with the same value returns the
    -- original instead of duplicating it. Mobile networks make this essential.
    client_message_id   UUID   NOT NULL,
    kind                TEXT   NOT NULL DEFAULT 'text' CHECK (kind IN
                          ('text','image','video','audio','voice_note','document','sticker',
                           'gif','location','contact','poll','system','call_event')),
    ciphertext          BYTEA,
    -- Envelope version so the crypto layer can evolve without a data migration.
    envelope_version    SMALLINT NOT NULL DEFAULT 1,
    body_plaintext      TEXT,
    -- Routing/UI metadata only. NEVER message content (Appendix 3).
    metadata            JSONB  NOT NULL DEFAULT '{}'::jsonb,
    reply_to_id         UUID   REFERENCES messages(id) ON DELETE SET NULL,
    forwarded_from_id   UUID   REFERENCES messages(id) ON DELETE SET NULL,
    forward_score       SMALLINT NOT NULL DEFAULT 0,  -- powers "forwarded many times"
    expires_at          TIMESTAMPTZ,                  -- disappearing messages
    edited_at           TIMESTAMPTZ,
    edit_count          SMALLINT NOT NULL DEFAULT 0,
    deleted_at          TIMESTAMPTZ,
    deleted_for_everyone BOOLEAN NOT NULL DEFAULT FALSE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (conversation_id, seq),
    CONSTRAINT system_messages_have_no_sender
        CHECK (kind <> 'system' OR sender_id IS NULL)
);

-- Idempotency. The single most important index in the schema for perceived
-- reliability on poor networks.
CREATE UNIQUE INDEX messages_idempotency_idx
    ON messages (conversation_id, sender_id, client_message_id);

-- The hot read path: newest-first keyset pagination. Never OFFSET.
CREATE INDEX messages_conversation_seq_idx ON messages (conversation_id, seq DESC);
CREATE INDEX messages_expiring_idx ON messages (expires_at) WHERE expires_at IS NOT NULL;

-- Edit history (spec §4). Ciphertext again — the server keeps versions it
-- cannot read.
CREATE TABLE message_revisions (
    id          UUID PRIMARY KEY,
    message_id  UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    revision    SMALLINT NOT NULL,
    ciphertext  BYTEA,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (message_id, revision)
);

-- Per-message receipts exist ONLY for group chats, where the sender needs to
-- see who specifically has read. Direct chats rely on the high-water marks in
-- conversation_members, which is far cheaper.
CREATE TABLE message_receipts (
    conversation_id UUID   NOT NULL,
    seq             BIGINT NOT NULL,
    user_id         UUID   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    delivered_at    TIMESTAMPTZ,
    read_at         TIMESTAMPTZ,
    played_at       TIMESTAMPTZ,   -- voice notes
    PRIMARY KEY (conversation_id, seq, user_id)
);

CREATE TABLE message_reactions (
    message_id  UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    emoji       TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, user_id, emoji)
);
CREATE INDEX message_reactions_message_idx ON message_reactions (message_id);

CREATE TABLE message_mentions (
    message_id      UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    mentioned_user_id UUID NOT NULL REFERENCES users(id)  ON DELETE CASCADE,
    PRIMARY KEY (message_id, mentioned_user_id)
);
-- Drives "@ me" filtering and the mention notification rule that overrides mute.
CREATE INDEX message_mentions_user_idx ON message_mentions (mentioned_user_id);

CREATE TABLE message_pins (
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    message_id      UUID NOT NULL REFERENCES messages(id)      ON DELETE CASCADE,
    pinned_by       UUID REFERENCES users(id) ON DELETE SET NULL,
    expires_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (conversation_id, message_id)
);

CREATE TABLE message_bookmarks (
    user_id     UUID NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    message_id  UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, message_id)
);

-- Tombstones. Other devices need to learn a message is gone; without this a
-- deletion that arrives while a device is offline is invisible forever.
CREATE TABLE message_deletion_events (
    id              UUID PRIMARY KEY,
    conversation_id UUID   NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    seq             BIGINT NOT NULL,
    deleted_by      UUID REFERENCES users(id) ON DELETE SET NULL,
    for_everyone    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX message_deletion_events_conv_idx ON message_deletion_events (conversation_id, created_at DESC);

-- ===========================================================================
-- 6. MEDIA
--
-- Bytes live in object storage; only metadata lives here (Appendix 3).
-- Media is encrypted client-side with a per-content key that the server never
-- sees — the key travels inside the message ciphertext.
-- ===========================================================================

CREATE TABLE media_assets (
    id              UUID PRIMARY KEY,
    owner_id        UUID REFERENCES users(id) ON DELETE SET NULL,
    storage_bucket  TEXT NOT NULL,
    storage_key     TEXT NOT NULL,
    mime_type       TEXT NOT NULL,
    byte_size       BIGINT NOT NULL,
    sha256          TEXT,
    width           INT,
    height          INT,
    duration_ms     INT,
    -- Encrypted thumbnail bytes, small enough to inline for a fast preview.
    thumbnail_blob  BYTEA,
    is_encrypted    BOOLEAN NOT NULL DEFAULT TRUE,
    -- Upload sessions start `pending`; a sweeper deletes orphans that never
    -- complete, otherwise failed uploads accumulate as unbilled storage.
    upload_status   TEXT NOT NULL DEFAULT 'pending'
                      CHECK (upload_status IN ('pending','complete','failed','deleted')),
    scan_status     TEXT NOT NULL DEFAULT 'pending'
                      CHECK (scan_status IN ('pending','clean','malicious','skipped')),
    expires_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ,
    UNIQUE (storage_bucket, storage_key)
);
CREATE INDEX media_assets_owner_idx   ON media_assets (owner_id, created_at DESC);
CREATE INDEX media_assets_pending_idx ON media_assets (created_at) WHERE upload_status = 'pending';

CREATE TABLE message_attachments (
    message_id  UUID NOT NULL REFERENCES messages(id)     ON DELETE CASCADE,
    media_id    UUID NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    position    SMALLINT NOT NULL DEFAULT 0,
    caption_ciphertext BYTEA,
    PRIMARY KEY (message_id, media_id)
);

ALTER TABLE users        ADD CONSTRAINT users_avatar_fk
    FOREIGN KEY (avatar_media_id) REFERENCES media_assets(id) ON DELETE SET NULL;
ALTER TABLE conversations ADD CONSTRAINT conversations_avatar_fk
    FOREIGN KEY (avatar_media_id) REFERENCES media_assets(id) ON DELETE SET NULL;
ALTER TABLE communities  ADD CONSTRAINT communities_avatar_fk
    FOREIGN KEY (avatar_media_id) REFERENCES media_assets(id) ON DELETE SET NULL;

-- ===========================================================================
-- 7. CHANNELS AND STATUS
-- ===========================================================================

CREATE TABLE channel_followers (
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(id)         ON DELETE CASCADE,
    notifications_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    followed_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (conversation_id, user_id)
);
CREATE INDEX channel_followers_user_idx ON channel_followers (user_id);

CREATE TABLE statuses (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('text','image','video','voice')),
    ciphertext      BYTEA,
    media_id        UUID REFERENCES media_assets(id) ON DELETE SET NULL,
    background_color TEXT,
    font            TEXT,
    -- 'contacts' | 'contacts_except' | 'only_share_with'; the concrete list
    -- lives in status_audience.
    audience_mode   TEXT NOT NULL DEFAULT 'contacts',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL
);
CREATE INDEX statuses_user_idx   ON statuses (user_id, created_at DESC);
CREATE INDEX statuses_expiry_idx ON statuses (expires_at);

CREATE TABLE status_audience (
    status_id UUID NOT NULL REFERENCES statuses(id) ON DELETE CASCADE,
    user_id   UUID NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    PRIMARY KEY (status_id, user_id)
);

CREATE TABLE status_views (
    status_id UUID NOT NULL REFERENCES statuses(id) ON DELETE CASCADE,
    viewer_id UUID NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    viewed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (status_id, viewer_id)
);

-- ===========================================================================
-- 8. CALLS (signalling metadata only — media rides the SFU, spec §29)
-- ===========================================================================

CREATE TABLE calls (
    id              UUID PRIMARY KEY,
    conversation_id UUID REFERENCES conversations(id) ON DELETE SET NULL,
    initiator_id    UUID REFERENCES users(id) ON DELETE SET NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('audio','video')),
    is_group        BOOLEAN NOT NULL DEFAULT FALSE,
    sfu_room_id     TEXT,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    answered_at     TIMESTAMPTZ,
    ended_at        TIMESTAMPTZ,
    end_reason      TEXT CHECK (end_reason IN
                      ('completed','missed','declined','busy','failed','cancelled','timeout')),
    -- Aggregated quality metrics; never per-packet data.
    quality_summary JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX calls_conversation_idx ON calls (conversation_id, started_at DESC);
CREATE INDEX calls_initiator_idx    ON calls (initiator_id, started_at DESC);

CREATE TABLE call_participants (
    call_id     UUID NOT NULL REFERENCES calls(id)   ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    device_id   UUID REFERENCES devices(id) ON DELETE SET NULL,
    state       TEXT NOT NULL DEFAULT 'ringing'
                  CHECK (state IN ('ringing','waiting_room','joined','left','declined','missed')),
    joined_at   TIMESTAMPTZ,
    left_at     TIMESTAMPTZ,
    PRIMARY KEY (call_id, user_id)
);

CREATE TABLE call_events (
    id          BIGSERIAL PRIMARY KEY,
    call_id     UUID NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
    user_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    event_type  TEXT NOT NULL,
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX call_events_call_idx ON call_events (call_id, created_at);

-- ===========================================================================
-- 9. NOTIFICATIONS (spec §16)
--
-- Three layers, checked in this order when a message arrives:
--   1. notification_tokens      — where to send (per device)
--   2. notification_preferences — global rules: quiet hours, previews, tone
--   3. conversation_notification_settings — per-conversation override: mute,
--                                            custom tone
-- A mention overrides mute; that rule lives in the application layer, but the
-- data it needs is all here.
-- ===========================================================================

CREATE TABLE notification_tokens (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    device_id       UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL CHECK (provider IN ('fcm','apns','web_push')),
    token           TEXT NOT NULL,
    -- APNs needs to know whether to use the sandbox gateway.
    environment     TEXT NOT NULL DEFAULT 'production' CHECK (environment IN ('production','sandbox')),
    -- VoIP pushes (PushKit) use a different token and priority than alerts.
    is_voip         BOOLEAN NOT NULL DEFAULT FALSE,
    -- Rotation and invalid-token cleanup (spec §16): providers tell us when a
    -- token dies; we mark it here rather than deleting, so we can measure it.
    failure_count   SMALLINT NOT NULL DEFAULT 0,
    invalidated_at  TIMESTAMPTZ,
    last_used_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, token)
);
CREATE INDEX notification_tokens_user_idx ON notification_tokens (user_id)
    WHERE invalidated_at IS NULL;
CREATE INDEX notification_tokens_device_idx ON notification_tokens (device_id);

-- Named tones, not raw filenames: clients ship the audio, the server stores
-- the identifier. Seeded below with the default set.
CREATE TABLE notification_tones (
    id           TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    category     TEXT NOT NULL CHECK (category IN ('message','group','call','status','system')),
    asset_name   TEXT NOT NULL,
    is_default   BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order   SMALLINT NOT NULL DEFAULT 0
);

CREATE TABLE notification_preferences (
    user_id                 UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    messages_enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    groups_enabled          BOOLEAN NOT NULL DEFAULT TRUE,
    calls_enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    status_enabled          BOOLEAN NOT NULL DEFAULT FALSE,
    channels_enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    reactions_enabled       BOOLEAN NOT NULL DEFAULT TRUE,
    security_alerts_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    -- 'full' shows sender and content, 'name_only' shows the sender,
    -- 'hidden' shows neither. Devices in `hidden` get a data-only push.
    preview_mode            TEXT NOT NULL DEFAULT 'full'
                              CHECK (preview_mode IN ('full','name_only','hidden')),
    -- Tone selection (spec §16)
    message_tone_id         TEXT REFERENCES notification_tones(id) ON DELETE SET NULL,
    group_tone_id           TEXT REFERENCES notification_tones(id) ON DELETE SET NULL,
    call_ringtone_id        TEXT REFERENCES notification_tones(id) ON DELETE SET NULL,
    vibration               TEXT NOT NULL DEFAULT 'default'
                              CHECK (vibration IN ('off','short','default','long')),
    in_app_sounds           BOOLEAN NOT NULL DEFAULT TRUE,
    high_priority           BOOLEAN NOT NULL DEFAULT FALSE,
    -- Quiet hours. Stored as local wall-clock minutes plus an IANA timezone so
    -- the window survives the user travelling and DST changes.
    quiet_hours_enabled     BOOLEAN NOT NULL DEFAULT FALSE,
    quiet_hours_start_min   SMALLINT CHECK (quiet_hours_start_min BETWEEN 0 AND 1439),
    quiet_hours_end_min     SMALLINT CHECK (quiet_hours_end_min   BETWEEN 0 AND 1439),
    quiet_hours_timezone    TEXT NOT NULL DEFAULT 'UTC',
    -- Calls may be allowed to break through quiet hours.
    quiet_hours_allow_calls BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE conversation_notification_settings (
    conversation_id  UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id          UUID NOT NULL REFERENCES users(id)         ON DELETE CASCADE,
    -- NULL = not muted. A timestamp far in the future = muted "always".
    muted_until      TIMESTAMPTZ,
    -- Even when muted, a direct @mention still notifies unless this is false.
    notify_on_mention BOOLEAN NOT NULL DEFAULT TRUE,
    -- Per-conversation custom sound (spec §16). NULL falls back to the global
    -- preference.
    tone_id          TEXT REFERENCES notification_tones(id) ON DELETE SET NULL,
    call_ringtone_id TEXT REFERENCES notification_tones(id) ON DELETE SET NULL,
    vibration        TEXT CHECK (vibration IN ('off','short','default','long')),
    preview_mode     TEXT CHECK (preview_mode IN ('full','name_only','hidden')),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (conversation_id, user_id)
);

-- Delivery ledger. Keeps push idempotent (a retried dispatch will not
-- re-notify) and gives real delivery-rate metrics instead of guesses.
CREATE TABLE notification_deliveries (
    id              BIGSERIAL PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id       UUID REFERENCES devices(id) ON DELETE CASCADE,
    conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
    message_seq     BIGINT,
    category        TEXT NOT NULL CHECK (category IN
                      ('message','mention','reply','group','call','missed_call','status',
                       'channel','device_link','security','reaction')),
    provider        TEXT,
    status          TEXT NOT NULL DEFAULT 'queued'
                      CHECK (status IN ('queued','sent','delivered','failed','suppressed')),
    -- Why we chose not to send: 'muted', 'quiet_hours', 'online', 'blocked'.
    suppressed_reason TEXT,
    provider_message_id TEXT,
    error           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, conversation_id, message_seq, device_id, category)
);
CREATE INDEX notification_deliveries_user_idx ON notification_deliveries (user_id, created_at DESC);

-- ===========================================================================
-- 10. MODERATION, ADMIN, SECURITY AUDIT
-- ===========================================================================

CREATE TABLE reports (
    id              UUID PRIMARY KEY,
    reporter_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    reported_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    conversation_id UUID REFERENCES conversations(id) ON DELETE SET NULL,
    -- Reported messages are copied here at report time, because the reporter
    -- can delete them afterwards and moderation would lose the evidence.
    evidence        JSONB NOT NULL DEFAULT '{}'::jsonb,
    category        TEXT NOT NULL CHECK (category IN
                      ('spam','harassment','csam','violence','fraud','impersonation',
                       'self_harm','other')),
    description     TEXT,
    status          TEXT NOT NULL DEFAULT 'open'
                      CHECK (status IN ('open','reviewing','actioned','dismissed')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at     TIMESTAMPTZ
);
CREATE INDEX reports_status_idx ON reports (status, created_at DESC);

CREATE TABLE admin_users (
    id           UUID PRIMARY KEY,
    user_id      UUID UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    email        CITEXT NOT NULL UNIQUE,
    -- Least privilege (spec §14): roles are additive and narrow.
    role         TEXT NOT NULL CHECK (role IN
                   ('support','moderator','security','engineer','superadmin')),
    mfa_enabled  BOOLEAN NOT NULL DEFAULT TRUE,
    is_active    BOOLEAN NOT NULL DEFAULT TRUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE moderation_actions (
    id           UUID PRIMARY KEY,
    admin_id     UUID REFERENCES admin_users(id) ON DELETE SET NULL,
    report_id    UUID REFERENCES reports(id) ON DELETE SET NULL,
    target_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action       TEXT NOT NULL CHECK (action IN
                   ('warn','restrict','suspend','ban','shadow_limit','unban','no_action')),
    reason       TEXT NOT NULL,
    expires_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX moderation_actions_target_idx ON moderation_actions (target_user_id, created_at DESC);

-- Append-only. Every privileged action lands here. Nothing in this table is
-- ever updated or deleted by application code.
CREATE TABLE admin_audit_logs (
    id           BIGSERIAL PRIMARY KEY,
    admin_id     UUID REFERENCES admin_users(id) ON DELETE SET NULL,
    action       TEXT NOT NULL,
    resource_type TEXT,
    resource_id  TEXT,
    ip_hash      TEXT,
    user_agent   TEXT,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX admin_audit_logs_admin_idx ON admin_audit_logs (admin_id, created_at DESC);

-- User-visible security timeline: new device linked, key changed, PIN changed,
-- session revoked. Contains no secrets and no message content (spec §28).
CREATE TABLE security_events (
    id           BIGSERIAL PRIMARY KEY,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id    UUID REFERENCES devices(id) ON DELETE SET NULL,
    event_type   TEXT NOT NULL CHECK (event_type IN
                   ('login','logout','device_linked','device_revoked','key_changed',
                    'pin_changed','pin_failed','passkey_added','passkey_removed',
                    'session_reuse_detected','suspicious_login','account_deactivated',
                    'two_step_enabled','two_step_disabled','backup_created')),
    severity     TEXT NOT NULL DEFAULT 'info' CHECK (severity IN ('info','warning','critical')),
    ip_hash      TEXT,
    user_agent   TEXT,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX security_events_user_idx ON security_events (user_id, created_at DESC);

-- ===========================================================================
-- 11. BACKUPS, FLAGS, EVENT OUTBOX
-- ===========================================================================

-- Metadata only. Backup contents are client-encrypted and live in object
-- storage; the server cannot decrypt them (spec §15).
CREATE TABLE backup_metadata (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    version         INT  NOT NULL,
    storage_key     TEXT NOT NULL,
    byte_size       BIGINT NOT NULL,
    checksum_sha256 TEXT NOT NULL,
    -- 'passkey' | 'password' | 'recovery_key' — how the client derived the
    -- backup key. The key itself never reaches the server.
    key_protection  TEXT NOT NULL CHECK (key_protection IN ('passkey','password','recovery_key')),
    message_count   BIGINT,
    media_included  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ,
    UNIQUE (user_id, version)
);

CREATE TABLE feature_flags (
    key           TEXT PRIMARY KEY,
    description   TEXT,
    is_enabled    BOOLEAN NOT NULL DEFAULT FALSE,
    rollout_percent SMALLINT NOT NULL DEFAULT 0
                     CHECK (rollout_percent BETWEEN 0 AND 100),
    -- Explicit allow-list for staged rollouts and internal dogfooding.
    enabled_user_ids UUID[] NOT NULL DEFAULT '{}',
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Transactional outbox. Written in the same transaction as the data change, so
-- a crash between commit and publish cannot silently lose a fan-out.
CREATE TABLE event_outbox (
    id            BIGSERIAL PRIMARY KEY,
    topic         TEXT  NOT NULL,
    partition_key TEXT,
    payload       JSONB NOT NULL,
    attempts      SMALLINT NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at  TIMESTAMPTZ
);
CREATE INDEX event_outbox_unpublished_idx ON event_outbox (id) WHERE published_at IS NULL;

-- ===========================================================================
-- 12. SEED DATA
-- ===========================================================================

INSERT INTO notification_tones (id, display_name, category, asset_name, is_default, sort_order) VALUES
    ('tone.message.default',  'NigChat',      'message', 'nigchat_message.caf',  TRUE,  1),
    ('tone.message.chime',    'Chime',        'message', 'chime.caf',            FALSE, 2),
    ('tone.message.pulse',    'Pulse',        'message', 'pulse.caf',            FALSE, 3),
    ('tone.message.drop',     'Drop',         'message', 'drop.caf',             FALSE, 4),
    ('tone.message.silent',   'Silent',       'message', 'silent.caf',           FALSE, 9),
    ('tone.group.default',    'Group',        'group',   'nigchat_group.caf',    TRUE,  1),
    ('tone.group.tap',        'Tap',          'group',   'tap.caf',              FALSE, 2),
    ('tone.call.default',     'NigChat Ring', 'call',    'nigchat_ring.caf',     TRUE,  1),
    ('tone.call.classic',     'Classic',      'call',    'classic_ring.caf',     FALSE, 2),
    ('tone.call.soft',        'Soft',         'call',    'soft_ring.caf',        FALSE, 3),
    ('tone.status.default',   'Status',       'status',  'status_update.caf',    TRUE,  1),
    ('tone.system.security',  'Security',     'system',  'security_alert.caf',   TRUE,  1);

INSERT INTO feature_flags (key, description, is_enabled, rollout_percent) VALUES
    ('calls.web',              'Browser-based voice and video calling',  FALSE, 0),
    ('calls.screen_share',     'Screen sharing during calls',            FALSE, 0),
    ('messages.disappearing',  'Disappearing messages',                  TRUE,  100),
    ('messages.edit',          'Edit sent messages',                     TRUE,  100),
    ('auth.passkeys',          'Passkey sign-in',                        FALSE, 0),
    ('backup.encrypted',       'End-to-end encrypted backups',           FALSE, 0),
    ('payments.enabled',       'Payments surface',                       FALSE, 0);
