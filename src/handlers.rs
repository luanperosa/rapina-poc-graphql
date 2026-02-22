use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use juniper::http::{GraphQLRequest, graphiql::graphiql_source};
use rapina::prelude::*;
use rapina::response::BoxBody;

use crate::{
    db::Pool,
    schemas::root::{Context, Schema, create_schema},
};

/// Shared state holding the DB pool and the GraphQL schema.
#[derive(Clone)]
pub struct GraphQLState {
    pub pool: Pool,
    pub schema: Arc<Schema>,
}

impl GraphQLState {
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            schema: Arc::new(create_schema()),
        }
    }
}

/// GraphQL endpoint (POST /graphql)
#[post("/graphql")]
pub async fn graphql(
    state: State<GraphQLState>,
    data: Json<GraphQLRequest>,
) -> http::Response<BoxBody> {
    let state = state.into_inner();
    let ctx = Context {
        db_pool: state.pool,
    };

    let res = data.into_inner().execute(&state.schema, &ctx).await;
    let json = serde_json::to_string(&res).unwrap();

    http::Response::builder()
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .unwrap()
}

/// GraphQL endpoint (GET /graphql) — supports query-param requests
#[get("/graphql")]
pub async fn graphql_get(
    state: State<GraphQLState>,
    query: Query<GraphQLRequest>,
) -> http::Response<BoxBody> {
    let state = state.into_inner();
    let ctx = Context {
        db_pool: state.pool,
    };

    let res = query.0.execute(&state.schema, &ctx).await;
    let json = serde_json::to_string(&res).unwrap();

    http::Response::builder()
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .unwrap()
}

/// GraphiQL UI (GET /graphiql)
#[get("/graphiql")]
pub async fn graphql_playground() -> http::Response<BoxBody> {
    let html = graphiql_source("/graphql", None);

    http::Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(html)))
        .unwrap()
}