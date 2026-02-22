use juniper::GraphQLInputObject;
use mysql::{Row, from_row};

use crate::schemas::root::Context;

/// User
#[derive(Default, Debug)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
}

impl User {
    pub(crate) fn from_row(row: Row) -> Self {
        let (id, name, email) = from_row(row);

        Self { id, name, email }
    }
}

#[juniper::graphql_object(Context = Context)]
impl User {
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

#[derive(GraphQLInputObject)]
#[graphql(description = "User Input")]
pub struct UserInput {
    pub name: String,
    pub email: String,
}
