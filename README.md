# ootle-utxo-monitor

A standalone reference app that monitors the **stealth UTXOs** of a Tari Ootle resource, decrypts each
output's viewable balance with the resource's ElGamal view key, tracks spend state, and serves the results
over a REST API and embedded web UI.

<img width="1209" height="596" alt="image" src="https://github.com/user-attachments/assets/1e1ea74c-1498-4569-b412-d7598b78fa89" />


## What it does

Given a stealth resource address whose view **secret** you hold, the monitor acts as a resource-wide
auditor:

1. **Stream** — tails the indexer's stealth UTXO update stream for the resource, per shard, resuming from a
   persisted cursor.
2. **Fetch** — resolves each `Unspent` frame (which carries only a `(tag, public_nonce)` fetch key) to the
   full output via a batched `/utxos/fetch`, yielding the commitment and the `viewable_balance` ciphertext.
3. **Decrypt** — recovers each output's plaintext value with the view secret against a memory-mapped
   value-lookup table (ElGamal reverse lookup), on a bounded `spawn_blocking` worker pool.
4. **Serve** — exposes view-key/resource management, a filterable UTXO ledger, and sync status over
   `/api`, with a single-page web UI on the same port.

The view key is a **read-only auditor key** — the monitor holds no spend key and never signs or spends.

## Domain notes

A few non-obvious facts shape the design:

- Stealth UTXOs exist only for `ResourceType::Stealth` (fungible / NFT / confidential resources hold
  balances in vaults, not UTXOs). The indexer's UTXO update stream is the only way to discover them.
- The stream is insufficient alone: an `Unspent` frame carries no commitment and no ciphertext, so each must
  be fetched. `Spent`/`Burnt` frames are keyed by `UtxoId`. The stream must be run with `unspent_only=false`
  to observe spends.
- The indexer **prunes** `viewable_balance` on spend, so an output spent before it is first fetched can never
  be valued — its value stays `NULL` (`decrypt_state = NoCiphertext`).
- The stream emits nothing for a shard with no updates, so the resume cursor must always enumerate every
  shard, or new outputs in an as-yet-unseen shard are never discovered.

## Usage

```
ootle-utxo-monitor \
  --network localnet \
  --indexer-url http://localhost:12500 \
  --value-lookup-file /path/to/value_lookup.bin \
  --database ./data/monitor.db \
  --bind 127.0.0.1:9000
```

Then open <http://127.0.0.1:9000>, add a view key (generate or import a hex secret), and add a stealth
resource address to monitor. Ingestion and decryption start immediately — no restart required.

The same actions are available from the CLI:

```
ootle-utxo-monitor ... view-key add --label auditor [--secret-hex <hex>]
ootle-utxo-monitor ... view-key list
ootle-utxo-monitor ... resource add resource_<hex>
ootle-utxo-monitor ... resource list
```

### Key flags

| Flag | Default | Purpose |
| --- | --- | --- |
| `--indexer-url` | — | Indexer to stream UTXO updates from (required) |
| `--network` | — | Ootle network: `localnet`, `esmeralda`, `mainnet`, … (required) |
| `--value-lookup-file` | — | mmap value-lookup table for ElGamal decryption (required) |
| `--database` | `./ootle-utxo-monitor.db` | sqlite database path |
| `--bind` | `127.0.0.1:9000` | Address for the UI + REST API |
| `--poll-interval` | `30` | Seconds between stream passes once caught up |
| `--decrypt-workers` | physical cores (max 8) | Concurrent decrypt workers |
| `--from-epoch` | `0` | Genesis backfill floor for newly added resources |

Flags may also be supplied via environment variables (`MONITOR_INDEXER_URL`, `MONITOR_NETWORK`,
`MONITOR_VALUE_LOOKUP_FILE`, `MONITOR_DATABASE`, `MONITOR_BIND`, `MONITOR_LOG_LEVEL`).

## Layout

- `crates/monitor-core` — library: sqlite storage, the ingestion pipeline, the decrypt pool, view-key and
  resource validation. Unit-tested in isolation (including a real ElGamal decrypt round-trip and a
  mock-driven ingestion loop).
- `crates/monitor` — the `ootle-utxo-monitor` binary: CLI, axum server, and the embedded web UI.
- `web/dist` — the single-file web UI, embedded into the binary at build time via `rust-embed`.

## Building

Standard cargo workspace:

```
cargo build --release -p ootle-utxo-monitor
cargo test
```

> **Note:** the Ootle dependencies are currently path-linked to a sibling `tari-ootle` checkout (see
> `Cargo.toml`), so a checkout of [`tari-project/tari-ootle`](https://github.com/tari-project/tari-ootle) is
> expected at `../dan` relative to this repository.

## License

BSD-3-Clause.
