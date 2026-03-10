use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};

const NUM_USERS: usize = 1_000;
const NUM_PRODUCTS: usize = 5_000;
use juniper::{EmptySubscription, FieldResult, RootNode, graphql_object};
use rapina_app::{
    mock::MockDb,
    schemas::async_gql::root::{AqSchema, build_schema},
};

// ─── Juniper mock schema ──────────────────────────────────────────────────────
//
// Defined inline here so the bench can use Arc<MockDb> instead of an r2d2 pool.
// Mirrors the structure of src/schemas/ as closely as possible.

struct JuniperContext {
    db: Arc<MockDb>,
}

impl juniper::Context for JuniperContext {}

struct JuniperUser {
    id: String,
    name: String,
    email: String,
}

#[graphql_object(Context = JuniperContext)]
impl JuniperUser {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn email(&self) -> &str {
        &self.email
    }
}

struct JuniperProduct {
    id: String,
    user_id: String,
    name: String,
    price: f64,
}

#[graphql_object(Context = JuniperContext)]
impl JuniperProduct {
    fn id(&self) -> &str {
        &self.id
    }
    fn user_id(&self) -> &str {
        &self.user_id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn price(&self) -> f64 {
        self.price
    }

    // Per-item nested resolver — mirrors the N+1 scenario from src/schemas/product.rs.
    // NOTE: async-graphql has a built-in DataLoader that can batch these in production,
    // but the mock benchmark can't show that benefit because there's no I/O to batch.
    fn user(&self, context: &JuniperContext) -> Option<JuniperUser> {
        context.db.find_user(&self.user_id).map(|u| JuniperUser {
            id: u.id.clone(),
            name: u.name.clone(),
            email: u.email.clone(),
        })
    }
}

struct JuniperQuery;

#[graphql_object(Context = JuniperContext)]
impl JuniperQuery {
    fn users(context: &JuniperContext) -> FieldResult<Vec<JuniperUser>> {
        Ok(context
            .db
            .users
            .iter()
            .map(|u| JuniperUser {
                id: u.id.clone(),
                name: u.name.clone(),
                email: u.email.clone(),
            })
            .collect())
    }

    fn user(context: &JuniperContext, id: String) -> FieldResult<Option<JuniperUser>> {
        Ok(context.db.find_user(&id).map(|u| JuniperUser {
            id: u.id.clone(),
            name: u.name.clone(),
            email: u.email.clone(),
        }))
    }

    fn products(context: &JuniperContext) -> FieldResult<Vec<JuniperProduct>> {
        Ok(context
            .db
            .products
            .iter()
            .map(|p| JuniperProduct {
                id: p.id.clone(),
                user_id: p.user_id.clone(),
                name: p.name.clone(),
                price: p.price,
            })
            .collect())
    }
}

struct JuniperMutation;

#[graphql_object(Context = JuniperContext)]
impl JuniperMutation {
    // No-op write — benchmarks mutation parsing/dispatch overhead only.
    fn create_user(
        _context: &JuniperContext,
        name: String,
        email: String,
    ) -> FieldResult<JuniperUser> {
        Ok(JuniperUser {
            id: "new".into(),
            name,
            email,
        })
    }
}

type JuniperSchema =
    RootNode<'static, JuniperQuery, JuniperMutation, EmptySubscription<JuniperContext>>;

fn build_juniper_schema() -> JuniperSchema {
    JuniperSchema::new(JuniperQuery, JuniperMutation, EmptySubscription::new())
}

// ─── Benchmark helpers ────────────────────────────────────────────────────────

async fn run_juniper(schema: &JuniperSchema, db: &Arc<MockDb>, query: &str) {
    let ctx = JuniperContext { db: Arc::clone(db) };
    let res = juniper::execute(query, None, schema, &juniper::Variables::new(), &ctx)
        .await
        .unwrap();
    std::hint::black_box(res);
}

async fn run_aq(schema: &AqSchema, query: &str) {
    let res = schema.execute(query).await;
    std::hint::black_box(res);
}

// ─── Benchmark groups ─────────────────────────────────────────────────────────

fn bench_list_users(c: &mut Criterion) {
    let db = MockDb::generate(NUM_USERS, NUM_PRODUCTS);
    let juniper_schema = build_juniper_schema();
    let aq_schema = build_schema(Arc::clone(&db));
    let query = "{ users { id name email } }";

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("list_users");

    group.bench_function("juniper", |b| {
        b.to_async(&rt).iter(|| run_juniper(&juniper_schema, &db, query));
    });
    group.bench_function("async_graphql", |b| {
        b.to_async(&rt).iter(|| run_aq(&aq_schema, query));
    });

    group.finish();
}

fn bench_nested_products(c: &mut Criterion) {
    let db = MockDb::generate(NUM_USERS, NUM_PRODUCTS);
    let juniper_schema = build_juniper_schema();
    let aq_schema = build_schema(Arc::clone(&db));
    // NOTE: This reflects per-resolver dispatch cost only. async-graphql's DataLoader
    // can batch N+1 queries in production, but there's no I/O to batch here.
    let query = "{ products { id name price user { id name } } }";

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("nested_products");

    group.bench_function("juniper", |b| {
        b.to_async(&rt).iter(|| run_juniper(&juniper_schema, &db, query));
    });
    group.bench_function("async_graphql", |b| {
        b.to_async(&rt).iter(|| run_aq(&aq_schema, query));
    });

    group.finish();
}

fn bench_single_item_query(c: &mut Criterion) {
    let db = MockDb::generate(NUM_USERS, NUM_PRODUCTS);
    let juniper_schema = build_juniper_schema();
    let aq_schema = build_schema(Arc::clone(&db));
    let query = r#"{ user(id: "u1") { id name email } }"#;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("single_item_query");

    group.bench_function("juniper", |b| {
        b.to_async(&rt).iter(|| run_juniper(&juniper_schema, &db, query));
    });
    group.bench_function("async_graphql", |b| {
        b.to_async(&rt).iter(|| run_aq(&aq_schema, query));
    });

    group.finish();
}

fn bench_mutation(c: &mut Criterion) {
    let db = MockDb::generate(NUM_USERS, NUM_PRODUCTS);
    let juniper_schema = build_juniper_schema();
    let aq_schema = build_schema(Arc::clone(&db));
    let query = r#"mutation { createUser(name: "T", email: "t@t.com") { id } }"#;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("mutation");

    group.bench_function("juniper", |b| {
        b.to_async(&rt).iter(|| run_juniper(&juniper_schema, &db, query));
    });
    group.bench_function("async_graphql", |b| {
        b.to_async(&rt).iter(|| run_aq(&aq_schema, query));
    });

    group.finish();
}

fn bench_schema_build(c: &mut Criterion) {
    let db = MockDb::generate(NUM_USERS, NUM_PRODUCTS);
    let mut group = c.benchmark_group("schema_build");

    group.bench_function("juniper", |b| {
        b.iter(|| std::hint::black_box(build_juniper_schema()));
    });
    group.bench_function("async_graphql", |b| {
        b.iter(|| std::hint::black_box(build_schema(Arc::clone(&db))));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_list_users,
    bench_nested_products,
    bench_single_item_query,
    bench_mutation,
    bench_schema_build,
);
criterion_main!(benches);
