# GraphQL Support in Rapina - Implementation Plan

## Context

Rapina needs first-class GraphQL support. So I created a Proof of Concept using Rapina and [Juniper](https://crates.io/crates/juniper). The POC works but relies on low-level manual wiring: handlers build raw `http::Response<BoxBody>`, there is no integration with Rapina's error envelope (`trace_id`), and authentication info is not passed into the GraphQL context.

---

## Plan: Juniper GraphQL Module

TL;DR — Add a `rapina::graphql` module that provides: a `GraphQLRequest` extractor, a `GraphQLResponse` responder, a context builder that bridges Rapina's `CurrentUser` and `trace_id` into Juniper, an error adapter between `rapina::Error` ↔ `juniper::FieldError`, and router helpers to mount GraphQL + GraphiQL routes in one call.

---

### Steps

#### 1. Create the module graphql

Add a new `graphql` module (rapina/src/graphql/) to the crate and re-export its public API from `rapina/src/lib.rs`.

#### 2. `GraphQLRequest` extractor — `rapina/src/graphql/request.rs`

Create a unified `GraphQLRequest` extractor implementing `FromRequest`. It inspects the HTTP method:
- **POST** → deserialize the JSON body into `juniper::http::GraphQLRequest` (same as `Json<GraphQLRequest>` today).
- **GET** → deserialize query parameters into `juniper::http::GraphQLRequest` (same as `Query<GraphQLRequest>` today).

This replaces the need for separate POST/GET handlers with different extractor types. A single handler can call `graphql_request.execute(&schema, &ctx).await`.

Also register the type name in `is_parts_only_extractor()` inside `rapina-macros/src/lib.rs` — since it consumes the body on POST, it is **not** parts-only (it implements `FromRequest`, not `FromRequestParts`). The macro already handles `FromRequest` types correctly, so no change needed there beyond ensuring the type is recognized.

#### 3. `GraphQLResponse` responder — `rapina/src/graphql/response.rs`

Create a `GraphQLResponse` struct that wraps `juniper::GraphQLResponse` (or the raw `serde_json::Value` from execution) and implements `IntoResponse`:
- Sets `Content-Type: application/json`.
- Sets HTTP status 200 for successful responses, 400 if the GraphQL result contains only errors.
- Serializes the Juniper response to JSON.

This eliminates the manual `http::Response::builder()` boilerplate from the POC's `handlers.rs`.

#### 4. `RapinaGraphQLContext` — `rapina/src/graphql/context.rs`

Define a trait and a concrete context builder that bridges Rapina's request-level data into Juniper:



Provide a built-in `DefaultGraphQLContext` that carries:
- `current_user: Option<CurrentUser>` — `None` when unauthenticated, `Some(...)` when JWT is present.
- `trace_id: String` — from Rapina's `RequestContext`, threaded into error extensions.
- Generic `state: Arc<AppState>` — so the user can `.get::<Pool>()` or any other registered state.

The context is constructed **inside the built-in handler** (step 6) by reading `CurrentUser` from request extensions (it's `Option` — won't fail on public endpoints) and `RequestContext` for the trace ID.

Users who need a custom context implement the `GraphQLContext` trait on their own type and gain the same auto-construction.

#### 5. Error bridging — `rapina/src/graphql/error.rs`

Provide conversions between the two error systems:

- **Rapina → Juniper:** `impl From<rapina::Error> for juniper::FieldError` — maps `Error::not_found(msg)` into a `FieldError` whose extensions include `{ "code": "NOT_FOUND", "trace_id": "..." }`.
- **Juniper → Rapina:** A helper `graphql_error(code, message)` that creates a `FieldError` pre-populated with Rapina's error code vocabulary (`BAD_REQUEST`, `UNAUTHORIZED`, `FORBIDDEN`, `NOT_FOUND`, etc.) and the current `trace_id`.
- **`IntoFieldError` trait:** A convenience trait similar to `IntoApiError` but for GraphQL:



This ensures GraphQL error responses carry `trace_id` for production debugging, consistent with Rapina's REST error envelope.

#### 6. Built-in handlers — `rapina/src/graphql/handler.rs`

Provide three ready-made handler functions (not macro-based, raw closures compatible with `Router::route()`):

- **`graphql_handler`** — Handles both POST and GET. Extracts `GraphQLRequest`, builds the context (step 4), executes the query against the schema from `State`, returns `GraphQLResponse`.
- **`graphiql_handler`** — Serves the GraphiQL HTML UI. Takes the GraphQL endpoint path as a parameter. Returns `Content-Type: text/html`.

These are generic over the user's schema and context types.

#### 7. Router extension — `rapina/src/graphql/router.rs`

Add a convenience method to `Router` (or a standalone builder) to mount everything in one call:



`.graphql(path, ...)` registers both `POST` and `GET` handlers at the given path.
`.graphiql(path, graphql_endpoint)` registers a `GET` handler serving the playground UI.

Alternatively, provide a `GraphQLRouter::new(schema).with_playground(true).build()` that returns a `Router` which can be merged via `.group()`:



#### 8. Update `rapina-macros` — `rapina-macros/src/lib.rs`

In `is_parts_only_extractor()` (line ~195), the function already recognizes `CurrentUser` as parts-only. No changes needed for the built-in handlers (they use raw closures). If we want users to use `GraphQLRequest` as a macro-extracted param, it will be correctly treated as a body-consuming extractor since its type name won't match any of the parts-only patterns — so **no macro changes required**.

#### 9. Re-export from `rapina::prelude` and `rapina::graphql`

In `rapina/src/lib.rs`, expose:

In `rapina/src/prelude.rs`, optionally re-export key types:



#### 10. Add `juniper` as a dependency to `rapina/Cargo.toml`

Add `juniper = "0.16"` to the rapina crate dependencies.

#### 11. Documentation and example

- Add a `rapina/examples/graphql.rs` example demonstrating the full setup with users/products, DB pool, auth-aware resolvers.
- Add a docs page at `docs/content/docs/core-concepts/graphql.md`.

---

### Verification

1. **Unit tests** in `rapina/src/graphql/` for each component:
   - `request.rs` — test POST JSON extraction, GET query-param extraction, malformed input returns 400.
   - `response.rs` — test `IntoResponse` serialization, status codes for success/error.
   - `context.rs` — test context construction with and without `CurrentUser`.
   - `error.rs` — test `From<Error> for FieldError` round-trips, `trace_id` inclusion.

2. **Integration tests** in `rapina/tests/graphql.rs`:
   - Mount a simple schema via `GraphQLRouter`, use `TestClient` to send queries/mutations.
   - Verify GraphiQL playground returns HTML.
   - Verify unauthenticated request → `current_user` is `None` in context.
   - Verify authenticated request (JWT header) → `current_user` is `Some(...)` in context.
   - Verify error responses include `trace_id` in GraphQL error extensions.

3. **Manual smoke test** with the POC app migrated to use the new API — the `rapina-app` should simplify from ~80 lines in `handlers.rs` to ~10 lines.

---

### Decisions

- **Core, not plugin:** GraphQL lives in the core crate. Juniper is a direct dependency. This keeps the developer experience simple — one `use rapina::graphql::*` import.
- **Juniper only:** No abstraction layer over GraphQL libraries. Direct Juniper types. If async-graphql support is needed later, it would be a second module (`rapina::async_graphql`), not a shared trait.
- **Per-resolver auth:** The `/graphql` endpoint is implicitly public (the `GraphQLRouter` registers it as a public route). `Option<CurrentUser>` flows into the Juniper context. Resolvers call `ctx.current_user().ok_or(...)` to enforce auth. This follows the GraphQL community convention where a single endpoint handles mixed public/private operations.
- **No subscriptions:** `EmptySubscription` only. WebSocket support deferred. The `RootNode` type parameter uses `EmptySubscription<Context>`.
- **Trace ID in errors:** Every `FieldError` produced via the Rapina helpers includes `trace_id` in the `extensions` object, matching the REST error envelope pattern.