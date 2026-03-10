use async_graphql::{InputObject, SimpleObject};

#[derive(SimpleObject, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(InputObject)]
pub struct UserInput {
    pub name: String,
    pub email: String,
}
