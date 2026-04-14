# GraphQL Support in Rapina — Implementation Plan

## Context

Rapina needs first-class GraphQL support. A Proof of Concept was built using Rapina with both [Juniper](https://crates.io/crates/juniper) and [async-graphql](https://crates.io/crates/async-graphql). The POC works but relies on low-level manual wiring: handlers build raw `http::Response<BoxBody>`, there is no integration with Rapina's error envelope (`trace_id`), and authentication info is not passed into the GraphQL context.

After review, **async-graphql** was chosen over Juniper. It has a more ergonomic derive-based API (`#[Object]`, `#[SimpleObject]`, `#[InputObject]`), native async execution, built-in `DataLoader` support, and a larger community. The implementation will live behind a **feature flag** (`rapina = { features = ["graphql"] }`) so REST-only users aren't penalized with extra compile time or binary size.

---

## Plan: async-graphql Module

TL;DR — Add a `rapina::graphql` module (gated behind `#[cfg(feature = "graphql")]`) that provides: a `GraphQLRequest` extractor, a `GraphQLResponse` responder, a context builder that bridges Rapina's `CurrentUser` and `trace_id` into async-graphql's `Context`, an error adapter between `rapina::Error` and async-graphql errors, and builder methods on `Rapina` to mount GraphQL + GraphiQL routes ergonomically.

---

## PR Breakdown

The implementation is split into incremental, self-contained PRs. Each one is reviewable and testable on its own.

| PR | Scope | Depends on |
|----|-------|------------|
| **#1** | `GraphQLRequest` extractor + `GraphQLResponse` responder | — |
| **#2** | `RapinaGraphQLContext` with auth/trace_id bridging + error adapter | PR #1 |
| **#3** | Router extensions (`.graphql()` / `.graphiql()` builder methods) | PR #1, #2 |
| **#4** | GraphiQL playground + docs page + example | PR #3 |

---

## Steps

### 1. Create the module and feature flag

Add a new `graphql` module (`rapina/src/graphql/`) gated behind a cargo feature:

```toml
# rapina/Cargo.toml
[features]
default = []
graphql = ["dep:async-graphql"]

[dependencies]
async-graphql = { version = "7", optional = true }
```

In `rapina/src/lib.rs`:

```rust
#[cfg(feature = "graphql")]
pub mod graphql;
```

Re-export key types from `rapina::graphql` so users import from one place.

**PR: #1**

---

### 2. `GraphQLRequest` extractor — `rapina/src/graphql/request.rs`

Create a unified `GraphQLRequest` extractor implementing `FromRequest`. It inspects the HTTP method:

- **POST** → deserialize the JSON body into `async_graphql::Request`.
- **GET** → deserialize query parameters (`query`, `variables`, `operationName`) into `async_graphql::Request`.

async-graphql's `Request` already supports serde deserialization, so the POST path is straightforward. For GET, we deserialize a helper struct from query params and convert it:

```rust
/// Intermediate struct for GET query-param extraction.
#[derive(Deserialize)]
struct GraphQLParams {
    query: String,
    #[serde(default)]
    variables: Option<String>,
    #[serde(default, rename = "operationName")]
    operation_name: Option<String>,
}
```

The extractor wraps an `async_graphql::Request` and exposes it for execution:

```rust
pub struct GraphQLRequest(pub async_graphql::Request);
```

Since it consumes the body on POST, it implements `FromRequest` (not `FromRequestParts`). The macro already handles `FromRequest` types correctly — no `rapina-macros` changes required.

**PR: #1**

---

### 3. `GraphQLResponse` responder — `rapina/src/graphql/response.rs`

Create a `GraphQLResponse` struct that wraps `async_graphql::Response` and implements `IntoResponse`:

```rust
pub struct GraphQLResponse(pub async_graphql::Response);

impl IntoResponse for GraphQLResponse {
    fn into_response(self) -> http::Response<BoxBody> {
        let body = serde_json::to_vec(&self.0).unwrap();
        let status = if self.0.is_err() {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::OK
        };
        http::Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body)))
            .unwrap()
    }
}
```

Key detail: `async_graphql::Response` has an `.is_err()` method that checks whether the response contains errors, making status selection clean.

This eliminates the manual `http::Response::builder()` boilerplate from the POC's `handlers.rs`.

**PR: #1**

---

### 4. `RapinaGraphQLContext` — `rapina/src/graphql/context.rs`

async-graphql uses `Context<'_>` with type-map data injection via `Schema::build().data(...)` or per-request `Request::data(...)`. Unlike Juniper (which passes a user-defined `Context` struct), async-graphql's context is an extensible type-map — resolvers call `ctx.data::<T>()` or `ctx.data_unchecked::<T>()` to retrieve values.

Rapina's bridge injects request-scoped data into each `async_graphql::Request` before execution:

```rust
/// Holds request-scoped data to be injected into the async-graphql context.
pub struct RapinaGraphQLContext {
    pub current_user: Option<CurrentUser>,
    pub trace_id: String,
}
```

The built-in handler (step 6) constructs this per-request and injects it:

```rust
let ctx = RapinaGraphQLContext {
    current_user,  // from request extensions (Option — None on public endpoints)
    trace_id,      // from Rapina's RequestContext
};

let request = gql_request.0.data(ctx);
let response = schema.execute(request).await;
```

Resolvers access it via the standard async-graphql pattern:

```rust
#[Object]
impl QueryRoot {
    async fn me(&self, ctx: &Context<'_>) -> Result<User> {
        let rapina_ctx = ctx.data::<RapinaGraphQLContext>()?;
        let user = rapina_ctx.current_user
            .as_ref()
            .ok_or_else(|| Error::new("Unauthorized"))?;
        // ...
    }
}
```

Users who need additional context data can insert it via `request.data(...)` — async-graphql's type-map is open for extension, no custom trait required.

**PR: #2**

---

### 5. Error bridging — `rapina/src/graphql/error.rs`

Provide conversions between the two error systems. async-graphql uses `async_graphql::Error` (with `.extend_with()` for extensions) rather than Juniper's `FieldError`:

- **Rapina → async-graphql:** `impl From<rapina::Error> for async_graphql::Error` — maps `Error::not_found(msg)` into an `async_graphql::Error` whose extensions include `{ "code": "NOT_FOUND", "trace_id": "..." }`.

```rust
impl From<rapina::Error> for async_graphql::Error {
    fn from(err: rapina::Error) -> Self {
        let code = err.error_code();  // e.g. "NOT_FOUND"
        async_graphql::Error::new(err.message())
            .extend_with(|_, e| {
                e.set("code", code);
            })
    }
}
```

- **Helper function:** `graphql_error(code, message, trace_id)` creates an `async_graphql::Error` pre-populated with Rapina's error code vocabulary and the current `trace_id`:

```rust
pub fn graphql_error(code: &str, message: impl Into<String>, trace_id: &str) -> async_graphql::Error {
    async_graphql::Error::new(message)
        .extend_with(|_, e| {
            e.set("code", code);
            e.set("trace_id", trace_id);
        })
}
```

- **`IntoGraphQLError` trait:** A convenience trait similar to `IntoApiError` but for GraphQL, allowing user-defined error types to convert cleanly:

```rust
pub trait IntoGraphQLError {
    fn into_graphql_error(self, trace_id: &str) -> async_graphql::Error;
}
```

This ensures GraphQL error responses carry `trace_id` for production debugging, consistent with Rapina's REST error envelope.

**PR: #2**

---

### 6. Built-in handlers — `rapina/src/graphql/handler.rs`

Provide ready-made handler functions (raw closures compatible with `Router::route()`):

- **`graphql_handler`** — Handles both POST and GET. Extracts `GraphQLRequest`, reads `CurrentUser` and `RequestContext` from request extensions, injects `RapinaGraphQLContext` into the async-graphql `Request`, executes against the schema from `State`, returns `GraphQLResponse`.

```rust
async fn graphql_handler(
    state: State<GraphQLSchema>,
    ctx: Option<CurrentUser>,
    request_ctx: RequestContext,
    gql_request: GraphQLRequest,
) -> GraphQLResponse {
    let rapina_ctx = RapinaGraphQLContext {
        current_user: ctx,
        trace_id: request_ctx.trace_id().to_string(),
    };
    let request = gql_request.0.data(rapina_ctx);
    GraphQLResponse(state.execute(request).await)
}
```

- **`graphiql_handler`** — Serves the GraphiQL HTML UI. async-graphql provides `async_graphql::http::GraphiQLSource` which generates the HTML:

```rust
async fn graphiql_handler() -> impl IntoResponse {
    let html = GraphiQLSource::build().endpoint("/graphql").finish();
    http::Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(html)))
        .unwrap()
}
```

The schema type is `async_graphql::Schema<Query, Mutation, EmptySubscription>` — generic over the user's `Query` and `Mutation` root types.

**PR: #3**

---

### 7. Router builder methods — `rapina/src/graphql/router.rs`

Add convenience methods directly on the `Rapina` builder to mount GraphQL routes, aligning with existing builder methods (`with_cors`, `with_rate_limit`, etc.):

```rust
Rapina::new()
    .graphql("/graphql", schema)
    .graphiql("/graphiql")
    .listen("127.0.0.1:3000")
    .await
```

Implementation:

- **`.graphql(path, schema)`** — Stores the schema as shared state and registers both `POST` and `GET` handlers at the given path. The schema is wrapped in `Arc` internally.
- **`.graphiql(path)`** — Registers a `GET` handler serving the GraphiQL playground UI. Infers the GraphQL endpoint from the previously registered `.graphql()` call or accepts an explicit override: `.graphiql_at("/graphiql", "/graphql")`.

Under the hood, `.graphql()` creates a `Router` fragment with the two routes, stores the schema as `State`, and merges it into the app's router — same pattern as `.router()` today.

**PR: #3**

---

### 8. Update `rapina-macros` — `rapina-macros/src/lib.rs`

No macro changes required. The built-in handlers use raw closures/functions compatible with `Router::route()`. The `GraphQLRequest` extractor implements `FromRequest` (body-consuming), which the macro already handles correctly — its type name won't match any parts-only patterns in `is_parts_only_extractor()`.

**PR: N/A — no changes**

---

### 9. Re-export from `rapina::graphql`

In `rapina/src/lib.rs` (behind `#[cfg(feature = "graphql")]`):

```rust
#[cfg(feature = "graphql")]
pub mod graphql;
```

The `rapina::graphql` module re-exports:

```rust
pub use self::request::GraphQLRequest;
pub use self::response::GraphQLResponse;
pub use self::context::RapinaGraphQLContext;
pub use self::error::{graphql_error, IntoGraphQLError};
```

Optionally re-export in `rapina::prelude` behind the feature gate:

```rust
#[cfg(feature = "graphql")]
pub use crate::graphql::{GraphQLRequest, GraphQLResponse, RapinaGraphQLContext};
```

**PR: #1 (initial), expanded in #2**

---

### 10. Add `async-graphql` as an optional dependency to `rapina/Cargo.toml`

```toml
[features]
default = []
graphql = ["dep:async-graphql"]

[dependencies]
async-graphql = { version = "7", optional = true }
```

**PR: #1**

---

### 11. Documentation and example

- Add a `rapina/examples/graphql.rs` example demonstrating the full setup: schema with `#[Object]` derives, DB pool via `ctx.data::<Pool>()`, auth-aware resolvers, builder-style mounting.
- Add a docs page at `docs/content/docs/core-concepts/graphql.md`.

The example should show the target DX:

```rust
use rapina::prelude::*;
use async_graphql::{Object, Context, EmptySubscription, Schema};

struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn hello(&self) -> &str {
        "Hello from Rapina + async-graphql!"
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .finish();

    Rapina::new()
        .graphql("/graphql", schema)
        .graphiql("/graphiql")
        .listen("127.0.0.1:3000")
        .await
}
```

**PR: #4**

---

## Verification

### Per-PR tests

1. **PR #1** — Unit tests in `rapina/src/graphql/`:
   - `request.rs` — test POST JSON extraction, GET query-param extraction, malformed input returns 400.
   - `response.rs` — test `IntoResponse` serialization, status 200 for success, 400 for errors via `.is_err()`.

2. **PR #2** — Unit tests:
   - `context.rs` — test `RapinaGraphQLContext` injection, resolvers can access `current_user` and `trace_id` via `ctx.data::<RapinaGraphQLContext>()`.
   - `error.rs` — test `From<rapina::Error> for async_graphql::Error`, verify `trace_id` and `code` appear in extensions.

3. **PR #3** — Integration tests in `rapina/tests/graphql.rs`:
   - Mount a schema via `.graphql()`, use `TestClient` to send queries/mutations.
   - Verify unauthenticated request → `current_user` is `None`.
   - Verify authenticated request (JWT header) → `current_user` is `Some(...)`.
   - Verify error responses include `trace_id` in GraphQL error extensions.

4. **PR #4** — Integration tests:
   - Verify GraphiQL playground returns HTML with correct endpoint URL.
   - Example compiles and runs.

### Manual smoke test

Migrate the POC `rapina-app` to use the new API. The app should simplify from ~80 lines in `handlers.rs` to roughly:

```rust
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let pool = get_db_pool();
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(pool)
        .finish();

    Rapina::new()
        .graphql("/graphql", schema)
        .graphiql("/graphiql")
        .listen("127.0.0.1:3000")
        .await
}
```

---

## Decisions

- **Feature flag, not core:** GraphQL lives in the main crate but behind `#[cfg(feature = "graphql")]`. The `async-graphql` dependency is optional. REST-only users pay zero cost.
- **async-graphql, not Juniper:** async-graphql has a more ergonomic API (`#[Object]`, `#[SimpleObject]`, `#[InputObject]` derives), native async, built-in `DataLoader`, and stronger community momentum. Direct async-graphql types — no abstraction layer.
- **Per-resolver auth:** The `/graphql` endpoint is implicitly public. `Option<CurrentUser>` is injected into async-graphql's context via `Request::data()`. Resolvers call `ctx.data::<RapinaGraphQLContext>()?.current_user.as_ref().ok_or(...)` to enforce auth. This follows the GraphQL convention where a single endpoint handles mixed public/private operations.
- **No subscriptions (for now):** `EmptySubscription` only. WebSocket support deferred to a follow-up. Focus on queries and mutations first.
- **Trace ID in errors:** Every error produced via Rapina's GraphQL helpers includes `trace_id` in the `extensions` object, matching the REST error envelope pattern.
- **Incremental PRs:** The feature is broken into 4 self-contained PRs, each reviewable and testable independently.