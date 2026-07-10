# ootle-utxo-monitor — Design

Status: **spec** (design locked; no code yet). Target repo: `~/tari/utxo-monitor`.

A standalone reference app that monitors the Ootle chain for the **stealth UTXOs of a given
resource**, decrypts each output's viewable balance with the resource's view key, and exposes the
results over a REST API + a Vite/React web UI. It is also a **dogfood for `ootle-rs`**: the
stream→fetch→decrypt seam becomes the first real UTXO API on `ootle-rs`.

---

## 1. Goals / non-goals

**Goals**
- Given a resource address whose ElGamal view key we hold, continuously discover every stealth UTXO
  of that resource, decrypt its value, track its spend state, and list/stream the results.
- Be a clean, readable reference for the stealth-UTXO stream→fetch→decrypt pipeline.
- Exercise `ootle-rs` and grow it where it is missing primitives.

**Non-goals**
- Not a wallet: no spending, no ownership/stealth-address matching, no key management beyond the
  view secret. The view key is used purely as a **resource-wide auditor** key.
- No consensus/reorg handling — the indexer is the source of truth.
- No auth (explicitly out of scope).

---

## 2. Domain background (the load-bearing facts)

These are non-obvious and shaped the whole design.

1. **Stealth UTXOs exist only for `ResourceType::Stealth`.** Fungible / NonFungible / Confidential
   resources hold balances in *vaults*, not UTXOs. A stealth UTXO is a `SubstateId::Utxo`
   substate, one per commitment, addressed `utxo_{resource}_{id}` where `id = UtxoId = Pedersen
   commitment bytes`.

2. **The indexer UTXO stream is the only way to monitor them.** The engine emits no `std.utxo.*`
   event on stealth-UTXO create/spend/burn (only vault/resource/component events exist), so the
   SSE event stream cannot see them. The relevant surface is the binary UTXO update stream
   (`tari_indexer_client::stream_utxo_updates_protobuf`).

3. **The stream alone is insufficient → a mandatory 3-stage pipeline.** The `Unspent` frame carries
   only `{ tag, public_nonce }` — **no commitment/`UtxoId` and no ciphertext**. To decrypt you need
   `OutputBody.viewable_balance` (the `ElgamalVerifiableBalanceBytes`), which lives only in the full
   `Utxo` substate. So:

   ```
   stream (discover + advance per-shard cursor)
     → POST /utxos/fetch  keyed by (resource, [(tag, public_nonce)])  → Vec<(UtxoId, Utxo)>
       → ElGamal decrypt (the expensive stage)
   ```

   `Spent{id,version}` / `Burnt{id,version}` frames *are* keyed by `UtxoId`. **`unspent_only=false`
   is required** to receive spend frames at all.

4. **The indexer prunes `viewable_balance` on spend.** An output that is created *and already spent*
   before we fetch it can never be valued — `/utxos/fetch` returns only currently-unspent outputs,
   and the ciphertext is gone once spent. Consequence: **`value` is nullable**, and a `Spent` row
   may legitimately have `value = NULL`. Live tailing has a tiny version of this race (created and
   spent inside one ~30s indexer window).

5. **Two-nonce gotcha.** The stream/`OutputBody.public_nonce` (the *fetch key*, paired with `tag`)
   is a **different point** from the ElGamal `public_nonce` (`R`, inside `viewable_balance`). Fetch
   with the former; decrypt with the latter.

6. **Resource-wide auditor semantics.** A stealth resource embeds `view_key: Option<RistrettoPublicKeyBytes>`
   (set at creation via `ResourceBuilder::stealth().with_view_key(pub)`). Whoever holds the *secret*
   can decrypt the `viewable_balance` of **every** output of that resource. The monitor therefore
   fetches and decrypts every `Unspent` frame; the `tag` is stored as opaque metadata only. No spend
   key, no per-account derivation.

7. **Decrypt failure is unambiguous here.** `ElgamalVerifiableBalance::decrypt` returns
   `Option<u64>` against a `ValueLookup`. The on-the-fly tail fallback was removed (computing `v·G`
   beyond the file range "takes years"), so `None` means *value outside the lookup file's coverage*.
   Because the app pre-verifies the view key against `resource.view_key()`, a `None` cannot be a
   key mismatch — it is a **retryable out-of-range** result (retry with a wider file).

---

## 3. Dependencies

Path-linked into the sibling `../dan` checkout for now (per project decision).

```toml
[dependencies]
ootle-rs                 = { path = "../dan/crates/wallet/ootle-rs" }
tari_indexer_client      = { path = "../dan/clients/tari_indexer_client", features = ["client"] }
tari_ootle_common_types  = { path = "../dan/crates/common_types" }        # Shard, StateVersion, NumPreshards, Epoch
tari_ootle_wallet_crypto = { path = "../dan/crates/wallet/crypto", features = ["mmap-value-lookup"] }
tari_engine_types        = { path = "../dan/crates/engine_types" }        # Resource, Utxo, crypto::elgamal, ValueLookup
tari_template_lib_types  = { path = "../dan/crates/template_lib_types" }  # ResourceAddress, UtxoAddress, UtxoId, UtxoTag, RistrettoPublicKeyBytes, Amount
tari_crypto              = "..."                                           # RistrettoSecretKey/PublicKey (match workspace pin)

axum = "..."         # single-port UI + /api + SSE
sqlx = { version = "...", features = ["sqlite", "runtime-tokio"] }
tokio = { version = "...", features = ["full"] }
rust-embed = "..."   # embed the built Vite dist
clap = { version = "...", features = ["derive", "env"] }
```

> Note the confirmed crate versions: `ootle-rs` 0.13.1, `tari_indexer_client` 0.34.1,
> `tari_ootle_wallet_crypto` 0.35.1, `tari_template_lib_types` 0.28.0.

---

## 4. `ootle-rs` additions (dogfooding)

Landed on `ootle-rs` *while building the app* (the app consumes them). The full `StealthUtxoMonitor`
pipeline graduation is deferred until the app proves the shape.

**#1 — typed UTXO update stream** (wraps `stream_utxo_updates_protobuf`, currently absent):

```rust
pub struct StealthUtxoWatchRequest {
    pub resource_address: ResourceAddress,
    pub from_epoch: Epoch,
    pub shard_state_versions: Vec<(Shard, StateVersion)>,
    pub unspent_only: bool,       // app always sets false (needs spends)
    pub per_shard_limit: u32,
}

pub enum StealthUtxoFrame {
    StartOfShard { shard: Shard, max_state_version: StateVersion, num_updates: u32 },
    Unspent { tag: UtxoTag, public_nonce: RistrettoPublicKeyBytes },
    Spent   { id: UtxoId, version: u32 },
    Burnt   { id: UtxoId, version: u32 },
    EndOfShard { shard: Shard, max_state_version: StateVersion },
}

impl IndexerProvider<W> {
    pub fn watch_stealth_utxos(&self, req: StealthUtxoWatchRequest)
        -> impl Stream<Item = ProviderResult<StealthUtxoFrame>>;
}
```

**#2 — batched fetch by (tag, nonce)** (wraps `POST /utxos/fetch`, currently absent; existing
`get_utxo_value` fetches one by `UtxoAddress` which we don't have):

```rust
impl IndexerProvider<W> {
    pub async fn fetch_unspent_utxos(
        &self,
        resource: ResourceAddress,
        pairs: &[(UtxoTag, RistrettoPublicKeyBytes)],   // ≤ 1000 per call
    ) -> ProviderResult<Vec<(UtxoId, Utxo)>>;
}
```

**#3 — per-UTXO decrypt with a rich outcome** (existing `decrypt_stealth_utxo_values` drops
non-covered outputs; we need the failure signal). Implemented returning a `StealthUtxoValue` per UTXO
(richer than the originally-sketched `Option<u64>`) so it maps 1:1 onto the app's `decrypt_state`:

```rust
pub enum StealthUtxoValue {
    Value(u64),        // -> decrypt_state = Decrypted, value = v
    OutOfRange,        // -> FailedOutOfRange (retryable with a wider lookup)
    NoViewableBalance, // -> Failed (hard)
    MalformedProof,    // -> Failed (hard)
}

impl IndexerProvider<W> {
    pub fn decrypt_stealth_utxo_values_opt<L: ValueLookup>(
        &self,
        view_secret: &RistrettoSecretKey,
        utxos: &[(UtxoId, Utxo)],
        lookup: &L,
    ) -> ProviderResult<Vec<(UtxoId, StealthUtxoValue)>>;   // input order preserved
}
```

**#4 — typed `get_resource`** (provider only has generic `fetch_substate -> Substate`):

```rust
impl IndexerProvider<W> {
    pub async fn get_resource(&self, addr: ResourceAddress) -> ProviderResult<Resource>;
}
```

**#6 — cursor helper**:

```rust
pub struct ShardCursor { /* Shard -> StateVersion */ }
impl ShardCursor {
    pub fn genesis(num_preshards: NumPreshards) -> Self;           // all shards @ StateVersion::zero
    pub fn to_pairs(&self) -> Vec<(Shard, StateVersion)>;
    pub fn observe(&mut self, shard: Shard, max_state_version: StateVersion);  // advance from EndOfShard
}
```

**#7 — feature forward**: add `mmap-value-lookup = ["tari_ootle_wallet_crypto/mmap-value-lookup"]`
to `ootle-rs` so `SortedPrefixFileLookup` is reachable as `ootle_rs::crypto::SortedPrefixFileLookup`.

**Not added:** the view-key/resource match check (`is_stealth && view_key==pub(secret)`) — stays
in-app, trivial from primitives.

---

## 5. Architecture

```
                        ┌──────────────────────── axum (single --bind) ───────────────────────┐
                        │   /            embedded Vite/React (rust-embed)                      │
   web browser ───────► │   /api/*       REST (view-keys, resources, utxos, status, reprocess) │
                        │   /api/events/stream   SSE "Live" feed (best-effort)                 │
                        └───────────────────────────────▲──────────────────────────────────────┘
                                                         │ reads sqlite / subscribes broadcast
 sqlite (sqlx) ◄─────────────────────────────────────────┤
   view_keys, resources, utxos, shard_cursors            │ writes
                                                         │
   per-resource task (one per active resource)           │        shared across all tasks:
   ┌──────────────────────────────────────────────┐      │        - Arc<SortedPrefixFileLookup>
   │ loop {                                        │──────┘        - Semaphore(N) decrypt permits
   │   frames = watch_stealth_utxos(cursor,        │               - sqlx pool
   │                 unspent_only=false)           │               - one IndexerProvider client
   │   for shard batch:                            │               - broadcast::Sender<UtxoChangeEvent>
   │     unspent = fetch_unspent_utxos(resource,   │
   │                  [(tag,nonce)])   ◄── inline  │
   │     persist rows Decrypting; enqueue decrypt  │
   │     apply Spent/Burnt by UtxoId               │
   │     advance shard_cursors (only on success)   │
   │   sleep(poll_interval)                        │
   │ }                                             │
   └──────────────────────────────────────────────┘
```

**Pipeline (Q1 = "fetch inline, then persist"):** the stream-drain loop collects each shard's
`(tag,nonce)` pairs, calls `fetch_unspent_utxos` immediately, and only then inserts rows — each born
with its real `UtxoId` PK, its ciphertext, `decrypt_state=Decrypting`, and enqueued to the decrypt
pool. The per-shard `StateVersion` cursor advances **only after** fetch+persist for that batch
succeeds, so a crash mid-batch simply re-streams it (idempotent upsert by PK).

**Concurrency (Q4):** decrypt runs via `tokio::task::spawn_blocking` gated by a global
`Semaphore(N)` (`N = --decrypt-workers`, default = physical cores capped at 8). Tokio's blocking
pool *is* the worker set; the semaphore caps real parallelism (and mmap/CPU contention) below the
512-thread ceiling, and also throttles the stream loop during backfill (it blocks acquiring
permits). Each resource task awaits its own chunk futures via `FuturesUnordered` and writes results
through the shared `sqlx` pool — no separate writer thread. Chunk size ~128 via `decrypt_many`.

**Multi-resource:** one supervised task per active resource; all share the single decrypt pool, so
decryption parallelism is pool-wide regardless of resource count. Disabling a resource cancels its
task and retains its ledger.

**Backfill (Q3):** new resources start from genesis — `ShardCursor::genesis(NumPreshards)` (all 256
shards @ `StateVersion::zero`), `from_epoch = --from-epoch` (default 0). One stream call drains all
shards (pass every shard's cursor; page with `per_shard_limit` + a `has_more` loop until every shard
is caught up), then sleep. Poll interval default ~30s (matches the indexer's validator sync cadence;
faster just yields empty drains).

---

## 6. Decryption

- Open the file once at startup: `let f = File::open(path)?; let lookup = Arc::new(unsafe { SortedPrefixFileLookup::load(&f)? });`
  Verify magic `VLKT`; capture `lookup.range()` for coverage checks. mmap is `Send + Sync`; share the
  one `Arc` across all workers (single mmap, page-cache backed).
- Per UTXO: `output.output.viewable_balance` → `ElgamalVerifiableBalance::convert_from_byte_type(&bytes)?`
  → `.decrypt(&view_secret, &*lookup)` (or `decrypt_many` for a chunk).
- `Some(v)` → `decrypt_state=Decrypted`, `value=v`.
- `None` → `decrypt_state=FailedOutOfRange`, stamp `fail_lookup_max = lookup.range().end()` (retryable).
- `viewable_balance == None` or malformed ciphertext → `decrypt_state=Failed` (hard invariant
  violation; should never happen for a viewable resource; log loudly, no retry).

---

## 7. Data model

sqlite via sqlx. Secrets stored plaintext (reference app — accepted).

```sql
CREATE TABLE view_keys (
    id         INTEGER PRIMARY KEY,
    label      TEXT NOT NULL,
    secret     BLOB NOT NULL,               -- 32-byte Ristretto scalar
    public     BLOB NOT NULL UNIQUE,        -- 32-byte compressed point = pub(secret)
    created_at TEXT NOT NULL
);

CREATE TABLE resources (
    resource_address TEXT PRIMARY KEY,       -- canonical string
    view_key_id      INTEGER NOT NULL REFERENCES view_keys(id),
    divisibility     INTEGER NOT NULL,       -- for value display
    from_epoch       INTEGER NOT NULL,       -- backfill floor used
    active           INTEGER NOT NULL DEFAULT 1,
    added_at         TEXT NOT NULL
);

CREATE TABLE utxos (
    resource_address     TEXT NOT NULL REFERENCES resources(resource_address),
    utxo_id              BLOB NOT NULL,      -- 32-byte commitment
    tag                  INTEGER NOT NULL,   -- UtxoTag (opaque metadata)
    fetch_nonce          BLOB,               -- OutputBody.public_nonce (fetch key; debug/re-fetch)
    ciphertext           BLOB,               -- viewable_balance (64B) for (re)decrypt
    value                INTEGER,            -- microtari, NULL until/without decrypt
    spend_state          TEXT NOT NULL,      -- Unspent | Spent | Burnt
    decrypt_state        TEXT NOT NULL,      -- Decrypting | Decrypted | Failed | FailedOutOfRange | NoCiphertext
    fail_lookup_max      INTEGER,            -- coverage max at last OutOfRange attempt (auto-retry)
    created_epoch        INTEGER,
    created_state_version INTEGER,
    spent_epoch          INTEGER,
    spent_state_version  INTEGER,
    first_seen_at        TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    PRIMARY KEY (resource_address, utxo_id)
);
CREATE INDEX idx_utxos_spend   ON utxos(resource_address, spend_state);
CREATE INDEX idx_utxos_decrypt ON utxos(resource_address, decrypt_state);

CREATE TABLE shard_cursors (
    resource_address TEXT NOT NULL REFERENCES resources(resource_address),
    shard            INTEGER NOT NULL,       -- 1..=256
    state_version    INTEGER NOT NULL,
    PRIMARY KEY (resource_address, shard)
);
```

**State model (Q6 — two orthogonal axes):**
- `spend_state ∈ {Unspent, Spent, Burnt}` — driven purely by stream frames (by `UtxoId`).
- `decrypt_state ∈ {Decrypting, Decrypted, Failed, FailedOutOfRange, NoCiphertext}` — driven purely
  by the worker (and ingestion for `NoCiphertext`).
- A `Spent`/`Burnt` frame for an unknown `utxo_id` (backfill of an output spent before we saw it
  unspent) inserts a row directly with `spend_state=Spent/Burnt`, `decrypt_state=NoCiphertext`,
  `ciphertext=NULL`, `value=NULL`. `NoCiphertext` is terminal (nothing to decrypt) — distinct from
  `Failed` (ciphertext present but malformed / invariant-violating) and `FailedOutOfRange`
  (ciphertext present, value beyond the lookup file's coverage).

**Add-resource validation (Q7):** `get_resource(addr)` → 3 gates, each a distinct error:
`resource.resource_type().is_stealth()` (else "not a stealth resource"); `view_key().is_some()`
(else "resource is not viewable"); `view_key() == Some(pub(secret))` for exactly one stored key
(else "you don't hold the view key for this resource"). On success link `view_key_id` + spawn task.
Idempotent on `resource_address`.

---

## 8. Retry / failure (Q5)

- Ciphertext is stored on the row, so retry never touches the indexer (and survives the indexer's
  spend-pruning).
- `FailedOutOfRange` is retryable: on startup, if the currently-configured lookup file's
  `range().end()` exceeds a row's `fail_lookup_max`, auto re-enqueue it (re-decrypt from stored
  ciphertext). Plus a manual `POST /api/reprocess-failed`.
- `Failed` (hard: malformed ciphertext / missing viewable_balance) is terminal — an invariant
  violation, logged, never retried.

---

## 9. REST API

Single-origin (served with the UI). All JSON under `/api`.

```
GET  /api/view-keys                     -> [{ id, label, public, created_at }]        (no secret)
POST /api/view-keys    {label, secret_hex?}   -> import if secret given else generate; returns public
GET  /api/view-keys/:id/secret          -> { secret_hex }                              (explicit reveal)
DELETE /api/view-keys/:id               -> 409 if any resource links it

GET  /api/resources                     -> [{ address, view_key_id, active, counts{by state}, sync }]
POST /api/resources    {resource_address}     -> 3-gate validate + link + spawn task
POST /api/resources/:addr/disable       -> cancel task, retain ledger
POST /api/resources/:addr/enable

GET  /api/utxos?resource=&spend_state=&decrypt_state=&utxo_id=&limit=&cursor=
        -> { items:[{ utxo_id, value, spend_state, decrypt_state, created_epoch, ... }], next_cursor }
        keyset pagination by (created_state_version, utxo_id); utxo_id= is exact (0/1 result)

POST /api/reprocess-failed   {resource?}      -> re-enqueue FailedOutOfRange

GET  /api/status
        -> { indexer_reachable, lookup:{path, range:{min,max}, num_records},
             workers:{n, in_flight}, resources:[{address, caught_up|backfilling, min_cursor,
             indexer_high_watermark, last_poll, counts}] }
```

- Values returned raw (`u64` microtari); UI formats with `resources.divisibility`.
- Pagination is **keyset**, not offset (a hot resource has many rows).

## 10. SSE "Live" page (Phase 2, best-effort)

- Ingestion publishes each transition onto an in-process `broadcast::Sender<UtxoChangeEvent>`
  `{ utxo_id, resource, kind: Created|Decrypted|Spent|Burnt|Failed, value?, epoch, state_version, ts }`.
- `GET /api/events/stream` subscribes a `BroadcastStream` and serializes SSE frames.
- **No** `Last-Event-ID` replay / dedup in v1: the persisted table + polled grid is the durable
  truth. Bounded (drop-oldest) broadcast; on reconnect the Live page reloads recent rows from
  `/api/utxos`. Main grid stays on polling — SSE is only the append-only activity feed.

---

## 11. CLI / config (clap + env; no TOML)

```
--indexer-url URL          (required)   which indexer to stream from
--value-lookup-file PATH   (required)   mmap SortedPrefixFileLookup; fail-fast on missing/bad magic
--network NAME             (required)   initializes the ootle-rs Wallet/provider (not cosmetic)
--database PATH            default ./ootle-utxo-monitor.db
--bind HOST:PORT           default 127.0.0.1:9000       (serves UI + /api + SSE)
--poll-interval SECS       default 30
--decrypt-workers N        default = physical cores (cap 8)
--per-shard-limit N        default 100
--from-epoch N             default 0 (genesis backfill floor for newly added resources)
--log-level ...
```

The lookup file is required because the app's entire job is decryption and the on-the-fly fallback
is non-viable; swap-and-restart is supported (Q8 auto-retry then fills previously out-of-range rows).

---

## 12. Operational defaults

- **Consistency:** trust the indexer's monotonic per-shard `StateVersion`; no reorg logic. If the
  indexer resets its DB, cursors reset with it (acceptable).
- **Resilience:** stream/fetch errors → exponential backoff in the per-resource loop; cursor
  advances only on a fully-successful batch; indexer-unreachable surfaced on `/api/status`.
- **View-key deletion:** blocked (409) while any resource links it.
- **Frontend build:** `npm run build` → Vite `dist` → `rust-embed` into the binary → single artifact.

---

## 13. Milestones

1. **ootle-rs primitives 1–4, 6, 7** (+ tests against a swarm indexer). App is built on these.
2. **Ingestion core:** per-resource task, cursor persistence, stream→fetch→persist, spend application.
   No decrypt yet (rows sit `Decrypting`). Verify counts/cursors against a live resource.
3. **Decrypt pool:** `spawn_blocking` + semaphore + `SortedPrefixFileLookup`; state transitions;
   retry/reprocess.
4. **REST API + status.**
5. **Web UI** (grid, filters, add view-key/resource flows, auto-refresh).
6. **SSE Live page** (Phase 2).

---

## Open notes
- Exact indexer route path for the stream (`/utxos/stream` vs `/utxos/updates`) is abstracted behind
  `tari_indexer_client::stream_utxo_updates_protobuf` — confirm at wire-up time; app never hardcodes it.
