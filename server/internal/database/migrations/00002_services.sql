-- +goose Up

-- Susunan ibadah tersimpan (Lentera 0.9), selain susunan utama di profiles.service. Seperti
-- materi, susunan yang dihapus tetap ada sebagai tanda hapus tanpa nama dan isi, dan revision
-- memakai penghitung profiles.revision yang sama. Susunan yang sedang dipakai tidak disimpan di
-- sini: itu milik perangkat.
CREATE TABLE services (
    profile_id uuid   NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    id         uuid   NOT NULL,
    name       text   NOT NULL CHECK (char_length(name) <= 40),
    -- Urutan id materi; materi yang sama boleh muncul lebih dari sekali.
    items      uuid[] NOT NULL DEFAULT '{}' CHECK (cardinality(items) <= 1000),
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL,
    deleted_at bigint,
    revision   bigint NOT NULL,
    PRIMARY KEY (profile_id, id),
    CHECK (CASE WHEN deleted_at IS NULL THEN char_length(name) >= 1
                ELSE name = '' AND cardinality(items) = 0 END)
);
CREATE INDEX services_profile_revision_idx ON services (profile_id, revision);

-- +goose Down
DROP TABLE services;
