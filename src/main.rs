use rapina::prelude::*;
use rapina::middleware::RequestLogMiddleware;

mod db;
mod handlers;
mod schemas;

use self::db::get_db_pool;
use self::handlers::{GraphQLState, graphql, graphql_get, graphql_playground};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let pool = get_db_pool();
    let gql_state = GraphQLState::new(pool);

    let router = Router::new()
        .post("/graphql", graphql)
        .get("/graphql", graphql_get)
        .get("/graphiql", graphql_playground);

    log::info!("starting HTTP server on port 3000");
    log::info!("GraphiQL playground: http://localhost:3000/graphiql");

    Rapina::new()
        .with_tracing(TracingConfig::new())
        .middleware(RequestLogMiddleware::new())
        .state(gql_state)
        .router(router)
        .listen("127.0.0.1:3000")
        .await
}