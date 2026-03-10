# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

This is a POC app used to prototype and validate a new GraphQL feature being added to the [Rapina](https://github.com/rapina-rs/rapina) Rust web framework. The goal is to prove out the integration patterns before implementing them in the framework itself.

## Commands

```bash
# Run the app (requires MySQL running)
cargo run

# Build release binary
cargo build --release

# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy

# Start MySQL container
docker compose up -d

# Seed DB manually (if volume already exists)
docker compose exec -T mysql mysql -u user -ppassword graphql_testing < mysql-schema.sql
```

The app listens on `http://localhost:3000`. GraphiQL playground: `http://localhost:3000/graphiql`.

## Architecture

This is a Rust POC demonstrating Rapina + Juniper GraphQL integration. It is intended as a reference for adding a `rapina::graphql` module to the Rapina framework itself (see `PLAN.md`).

**Request flow:**
```
HTTP request → main.rs router
  → handlers.rs (POST /graphql | GET /graphql | GET /graphiql)
    → Juniper executes schema with Context{db_pool}
      → schemas/root.rs (QueryRoot / MutationRoot)
        → schemas/user.rs or schemas/product.rs resolvers
          → MySQL via r2d2 pool
```

**Key files:**
- `src/main.rs` — wires router, state (`GraphQLState` = pool + compiled schema), and middleware
- `src/handlers.rs` — three HTTP handlers; manually builds `http::Response<BoxBody>` (intentionally low-level; PLAN.md outlines abstracting this into Rapina)
- `src/db.rs` — MySQL r2d2 connection pool setup
- `src/schemas/root.rs` — `Context` struct, `QueryRoot`, `MutationRoot` (all entry points for GraphQL ops)
- `src/schemas/product.rs` — nested `user` field resolver (issues a second DB query per product — N+1 risk)

**State threading:** `GraphQLState` (schema + pool) is registered with Rapina and extracted in handlers via `State<GraphQLState>`. Context is created per-request from the pool and passed into Juniper execution.

**Database:** MySQL 8.0, two tables — `user` and `product` (FK `product.user_id → user.id`). Schema and seed data in `mysql-schema.sql`. Connection string from `.env` (`DATABASE_URL`).

## Next steps (PLAN.md)

The planned `rapina::graphql` module will replace the manual handler boilerplate with:
- A unified `GraphQLRequest` extractor (handles POST JSON + GET query params)
- A `GraphQLResponse` responder (sets `Content-Type`, maps error status)
- `DefaultGraphQLContext` bridging `CurrentUser` + `trace_id` from Rapina into Juniper
- Error adapter: `rapina::Error` ↔ `juniper::FieldError` with `trace_id` in extensions
- `GraphQLRouter` convenience builder to mount GraphQL + GraphiQL in one call

Auth model: `/graphql` is a public route; resolvers enforce auth via `ctx.current_user().ok_or(...)`.
