# Juniper vs async-graphql Benchmark Results

Measured on 2026-03-10. All benchmarks use in-memory mock data (no live DB) so
results isolate library overhead from I/O. Both libraries run under the same
Tokio runtime via criterion's `async_tokio` runner.

Dataset: **1 000 users, 5 000 products** (products assigned to users in
round-robin). Controlled via `NUM_USERS` / `NUM_PRODUCTS` constants in
`benches/graphql_bench.rs`.

Machine: Linux 6.14, Rust 1.93.1 (release profile).

---

## Running the benchmarks

No database or external services needed — all benchmarks use in-memory mock data.

```bash
# Run all benchmark groups
cargo bench

# Run a single group by name
cargo bench -- list_users
cargo bench -- nested_products
cargo bench -- single_item_query
cargo bench -- mutation
cargo bench -- schema_build

# Save a named baseline (useful for comparing before/after a code change)
cargo bench -- --save-baseline my_baseline

# Compare against a previously saved baseline
cargo bench -- --baseline my_baseline
```

HTML reports with charts are generated automatically at:
```
target/criterion/<group>/<variant>/report/index.html
```

---

## Results

| Group | Juniper (mean) | async-graphql (mean) | Δ |
|---|---|---|---|
| `list_users` | 2.68 ms | 1.53 ms | **−43% (aq faster)** |
| `nested_products` | 34.0 ms | 24.6 ms | **−28% (aq faster)** |
| `single_item_query` | 10.1 µs | 13.4 µs | +33% |
| `mutation` | 8.33 µs | 11.7 µs | +40% |
| `schema_build` | 32.9 µs | 59.5 µs | +81% |

100 samples, ~3 s warmup per group (criterion extended automatically for slow
groups). Outlier counts were low (2–16 per 100 samples).

---

## Group notes

**`list_users` — `{ users { id name email } }`**
Flat traversal of 1 000 users. async-graphql is **43% faster** (1.53 ms vs
2.68 ms). At small row counts the two libraries were tied; the gap opens at
scale, likely due to async-graphql's more efficient internal value serialization
when building large response trees.

**`nested_products` — `{ products { id name price user { id name } } }`**
5 000 products, each with a nested user lookup (linear scan per product —
N+1 on the mock store). async-graphql is **28% faster** (24.6 ms vs 34.0 ms).

> **Important caveat:** async-graphql ships a built-in `DataLoader` that batches
> N+1 field resolvers into a single query. This benchmark cannot reflect that
> advantage because there is no I/O to batch in the mock. In production with a
> real DB the async-graphql DataLoader would outperform the Juniper N+1 pattern
> by a far larger margin than the 28% shown here.

**`single_item_query` — `{ user(id: "u1") { id name email } }`**
Single argument lookup regardless of dataset size. Juniper is ~33% faster
(10.1 µs vs 13.4 µs). The gap is consistent with the small-data runs and likely
reflects async-graphql's heavier argument deserialization machinery per call.

**`mutation` — `mutation { createUser(name: "T", email: "t@t.com") { id } }`**
No-op write; measures mutation parsing and dispatch only. Juniper is ~40% faster
(8.3 µs vs 11.7 µs), consistent with the single-item pattern above.

**`schema_build`**
async-graphql takes ~1.8× longer to initialize a schema (59.5 µs vs 32.9 µs).
In practice schemas are built once at startup, so this rarely matters for
throughput. It would affect test suite startup time if schemas are rebuilt per
test.

---

## Takeaways for `rapina::graphql`

1. **async-graphql wins at scale for list/collection queries.** It is 43% faster
   on `list_users` and 28% faster on `nested_products` at 1 000/5 000 rows. The
   gap was invisible with 2 rows and grows with dataset size, pointing to more
   efficient internal serialization when building large response trees.

2. **Juniper wins on per-call overhead.** Single-item lookups and mutations are
   33–40% faster in Juniper. This edge is constant (it doesn't shrink with more
   data), but the absolute difference is ~3–4 µs — well below any network or DB
   latency floor.

3. **async-graphql's DataLoader is the decisive production factor for relational
   data.** The `nested_products` mock benchmark already shows async-graphql
   faster at the pure dispatch level; with a real DB and DataLoader batching the
   N+1 lookups into a single query, the production advantage would be far larger.

4. **Schema initialization cost is a non-issue at runtime** but async-graphql's
   ~1.8× slower build time is worth noting for integration test setups that
   reconstruct schemas per test.

5. **Ecosystem fit:** async-graphql is actively maintained, has native `async`
   resolvers (no `execute_sync` fallback needed), and aligns naturally with
   Tokio. Juniper's sync-first design requires more boilerplate to integrate with
   an async framework like Rapina.
