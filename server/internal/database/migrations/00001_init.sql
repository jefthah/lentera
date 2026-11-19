-- +goose Up

-- Akun dan login -------------------------------------------------------------

-- Satu akun per alamat email. Belum ada kata sandi: masuk selalu lewat kode email.
CREATE TABLE accounts (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    email       text        NOT NULL UNIQUE CHECK (email = lower(email) AND length(email) <= 255),
    created_at  timestamptz NOT NULL DEFAULT now(),
    disabled_at timestamptz
);

-- Kode OTP masuk: satu kode aktif per email. Yang disimpan hanya HMAC-nya.
CREATE TABLE login_codes (
    email      text        PRIMARY KEY CHECK (email = lower(email)),
    code_hash  bytea       NOT NULL,
    attempts   smallint    NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    sent_at    timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);

-- Sesi perangkat. Aplikasi memegang token acak; database hanya menyimpan hash SHA-256-nya.
CREATE TABLE sessions (
    token_hash   bytea       PRIMARY KEY,
    account_id   bigint      NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    device       text        NOT NULL DEFAULT '' CHECK (char_length(device) <= 100),
    user_agent   text,
    ip_address   inet,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL
);
CREATE INDEX sessions_account_id_idx ON sessions (account_id);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

-- Profil dan pustaka ---------------------------------------------------------

-- Id profil dibuat aplikasi (UUID v7) dan sama dengan nama folder profil di perangkat.
-- Waktu dari aplikasi disimpan sebagai milidetik sejak 1970 (UTC), sama seperti di SQLite.
-- revision naik setiap ada perubahan di profil ini; aplikasi menyimpannya sebagai cursor.
CREATE TABLE profiles (
    id                  uuid        PRIMARY KEY,
    name                text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 40),
    name_updated_at     bigint      NOT NULL,
    created_at          bigint      NOT NULL,
    created_by          bigint      REFERENCES accounts (id) ON DELETE SET NULL,
    revision            bigint      NOT NULL DEFAULT 0,
    -- Susunan ibadah: urutan id materi; materi yang sama boleh muncul lebih dari sekali.
    service             uuid[]      NOT NULL DEFAULT '{}',
    service_updated_at  bigint      NOT NULL DEFAULT 0,
    service_revision    bigint      NOT NULL DEFAULT 0,
    -- Pilihan monitor tidak disinkronkan: itu milik perangkat, bukan profil.
    background          smallint    NOT NULL DEFAULT 0 CHECK (background BETWEEN 0 AND 15),
    settings_updated_at bigint      NOT NULL DEFAULT 0,
    settings_revision   bigint      NOT NULL DEFAULT 0,
    server_created_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at          timestamptz
);

-- Siapa boleh memakai profil. Satu akun bisa punya banyak profil, satu profil bisa dipakai
-- beberapa akun (misalnya tim multimedia satu gereja).
CREATE TABLE profile_members (
    profile_id uuid        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    account_id bigint      NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    role       text        NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    added_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (profile_id, account_id)
);
CREATE INDEX profile_members_account_id_idx ON profile_members (account_id);

-- Materi (lirik / ayat). Materi yang dihapus tetap ada sebagai tanda hapus tanpa judul dan isi,
-- supaya penghapusan ikut tersinkron ke perangkat lain.
CREATE TABLE items (
    profile_id uuid   NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    id         uuid   NOT NULL,
    kind       text   NOT NULL CHECK (kind IN ('song', 'verse')),
    title      text   NOT NULL,
    content    text   NOT NULL,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL,
    deleted_at bigint,
    revision   bigint NOT NULL,
    PRIMARY KEY (profile_id, id),
    CHECK (deleted_at IS NULL OR (title = '' AND content = ''))
);
CREATE INDEX items_profile_revision_idx ON items (profile_id, revision);

-- +goose Down
DROP TABLE items;
DROP TABLE profile_members;
DROP TABLE profiles;
DROP TABLE sessions;
DROP TABLE login_codes;
DROP TABLE accounts;
