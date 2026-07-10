CREATE TABLE view_keys (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    label      TEXT NOT NULL,
    secret     BLOB NOT NULL,               -- 32-byte Ristretto scalar
    public     BLOB NOT NULL UNIQUE,        -- 32-byte compressed point = pub(secret)
    created_at TEXT NOT NULL
);

CREATE TABLE resources (
    resource_address TEXT PRIMARY KEY,      -- canonical string
    view_key_id      INTEGER NOT NULL REFERENCES view_keys(id),
    divisibility     INTEGER NOT NULL,      -- for value display
    from_epoch       INTEGER NOT NULL,      -- backfill floor used
    active           INTEGER NOT NULL DEFAULT 1,
    added_at         TEXT NOT NULL
);

CREATE TABLE utxos (
    resource_address      TEXT NOT NULL REFERENCES resources(resource_address),
    utxo_id               BLOB NOT NULL,    -- 32-byte commitment
    -- tag + fetch_nonce are unknowable for an output first seen via a Spent/Burnt frame
    -- (the indexer has already pruned it), hence nullable.
    tag                   INTEGER,          -- UtxoTag (opaque metadata)
    fetch_nonce           BLOB,             -- OutputBody.public_nonce (fetch key; debug/re-fetch)
    ciphertext            BLOB,             -- viewable_balance (64B: encrypted || public_nonce)
    value                 INTEGER,          -- microtari, NULL until/without decrypt
    spend_state           TEXT NOT NULL,    -- Unspent | Spent | Burnt
    decrypt_state         TEXT NOT NULL,    -- Decrypting | Decrypted | Failed | FailedOutOfRange | NoCiphertext
    fail_lookup_max       INTEGER,          -- coverage max at last OutOfRange attempt (auto-retry)
    created_epoch         INTEGER,
    created_state_version INTEGER,
    spent_epoch           INTEGER,
    spent_state_version   INTEGER,
    first_seen_at         TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    PRIMARY KEY (resource_address, utxo_id)
);
CREATE INDEX idx_utxos_spend   ON utxos (resource_address, spend_state);
CREATE INDEX idx_utxos_decrypt ON utxos (resource_address, decrypt_state);

CREATE TABLE shard_cursors (
    resource_address TEXT NOT NULL REFERENCES resources(resource_address),
    shard            INTEGER NOT NULL,
    state_version    INTEGER NOT NULL,
    PRIMARY KEY (resource_address, shard)
);
