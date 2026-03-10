use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct MockUser {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug)]
pub struct MockProduct {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub price: f64,
}

pub struct MockDb {
    pub users: Vec<MockUser>,
    pub products: Vec<MockProduct>,
}

impl MockDb {
    /// Returns a pre-seeded in-memory store matching the mysql-schema.sql seed data.
    pub fn seeded() -> Arc<Self> {
        Arc::new(Self {
            users: vec![
                MockUser {
                    id: "u1".into(),
                    name: "Alice".into(),
                    email: "alice@example.com".into(),
                },
                MockUser {
                    id: "u2".into(),
                    name: "Bob".into(),
                    email: "bob@example.com".into(),
                },
            ],
            products: vec![
                MockProduct {
                    id: "p1".into(),
                    user_id: "u1".into(),
                    name: "Widget".into(),
                    price: 29.0,
                },
                MockProduct {
                    id: "p2".into(),
                    user_id: "u2".into(),
                    name: "Gadget".into(),
                    price: 49.0,
                },
            ],
        })
    }

    /// Generates a large in-memory store for benchmarking.
    /// Products are assigned to users in round-robin order.
    pub fn generate(num_users: usize, num_products: usize) -> Arc<Self> {
        let users = (0..num_users)
            .map(|i| MockUser {
                id: format!("u{i}"),
                name: format!("User {i}"),
                email: format!("user{i}@example.com"),
            })
            .collect::<Vec<_>>();

        let products = (0..num_products)
            .map(|i| MockProduct {
                id: format!("p{i}"),
                user_id: format!("u{}", i % num_users),
                name: format!("Product {i}"),
                price: 10.0 + (i % 990) as f64,
            })
            .collect();

        Arc::new(Self { users, products })
    }

    pub fn find_user(&self, id: &str) -> Option<&MockUser> {
        self.users.iter().find(|u| u.id == id)
    }
}
