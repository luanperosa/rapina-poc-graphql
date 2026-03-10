use std::sync::Arc;

use async_graphql::{Context, Object};

use crate::mock::{MockDb, MockProduct};

use super::user::User;

pub struct Product(pub MockProduct);

#[Object]
impl Product {
    async fn id(&self) -> &str {
        &self.0.id
    }

    async fn user_id(&self) -> &str {
        &self.0.user_id
    }

    async fn name(&self) -> &str {
        &self.0.name
    }

    async fn price(&self) -> f64 {
        self.0.price
    }

    // Per-item nested resolver — mirrors the N+1 scenario from src/schemas/product.rs.
    // NOTE: async-graphql has a built-in DataLoader that can batch these in production,
    // but the mock benchmark can't show that benefit because there's no I/O to batch.
    async fn user(&self, ctx: &Context<'_>) -> Option<User> {
        let db = ctx.data_unchecked::<Arc<MockDb>>();
        db.find_user(&self.0.user_id).map(|u| User {
            id: u.id.clone(),
            name: u.name.clone(),
            email: u.email.clone(),
        })
    }
}
