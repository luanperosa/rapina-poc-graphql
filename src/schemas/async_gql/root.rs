use std::sync::Arc;

use async_graphql::{Context, EmptySubscription, Object, Schema};

use crate::mock::MockDb;

use super::product::Product;
use super::user::User;

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn users(&self, ctx: &Context<'_>) -> Vec<User> {
        let db = ctx.data_unchecked::<Arc<MockDb>>();
        db.users
            .iter()
            .map(|u| User {
                id: u.id.clone(),
                name: u.name.clone(),
                email: u.email.clone(),
            })
            .collect()
    }

    async fn user(&self, ctx: &Context<'_>, id: String) -> Option<User> {
        let db = ctx.data_unchecked::<Arc<MockDb>>();
        db.find_user(&id).map(|u| User {
            id: u.id.clone(),
            name: u.name.clone(),
            email: u.email.clone(),
        })
    }

    async fn products(&self, ctx: &Context<'_>) -> Vec<Product> {
        let db = ctx.data_unchecked::<Arc<MockDb>>();
        db.products.iter().map(|p| Product(p.clone())).collect()
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    // No-op write — benchmarks mutation parsing/dispatch overhead only.
    async fn create_user(
        &self,
        _ctx: &Context<'_>,
        name: String,
        email: String,
    ) -> User {
        User {
            id: "new".into(),
            name,
            email,
        }
    }
}

pub type AqSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema(db: Arc<MockDb>) -> AqSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(db)
        .finish()
}
