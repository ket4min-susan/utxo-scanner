-- Per-row decrypt cost. Ciphertexts are decrypted in batches (one `decrypt_many` lookup per
-- chunk), so this is the chunk's elapsed time divided by its rows, not an isolated measurement.
-- NULL until a decrypt attempt is recorded, and cleared when a row is requeued for retry.
ALTER TABLE utxos ADD COLUMN decrypt_micros INTEGER;
