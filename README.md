# Rapina GraphQL POC

## Getting Started

### 1. Clone the repository

```bash
git clone [rapina-poc-graphql](https://github.com/luanperosa/rapina-poc-graphql) && cd rapina-poc-graphql
```

### 2. Configure environment variables

Copy the example file and adjust if needed:

```bash
cp .env.example .env
```

### 3. Start the database

```bash
docker compose up -d
```

This starts a MySQL 8.0 container with a health check. The schema file is mounted to `/docker-entrypoint-initdb.d/`, so if the volume is fresh the database will be seeded automatically.

### 4. Seed the database (if the volume already exists)

If the container was already initialized from a previous run, the auto-init script won't re-execute. Re-seed manually:

```bash
docker compose exec -T mysql mysql -u user -ppassword graphql_testing < mysql-schema.sql
```

### 5. Run the application

```bash
cargo run
```

### 6. Open the GraphiQL playground

Navigate to **http://localhost:3000/graphiql** in your browser.

## API Reference

**User**

| Field | Type | Description |
|---|---|---|
| `id` | `String!` | Unique identifier |
| `name` | `String!` | User's name |
| `email` | `String!` | User's email (unique) |

**Product**

| Field | Type | Description |
|---|---|---|
| `id` | `String!` | Unique identifier |
| `userId` | `String!` | ID of the owning user |
| `name` | `String!` | Product name |
| `price` | `Float!` | Product price |
| `user` | `User` | The owning user (resolved via DB lookup) |

#### Queries

| Query | Arguments | Returns | Description |
|---|---|---|---|
| `users` | — | `[User!]!` | List all users |
| `user` | `id: String!` | `User!` | Get a user by ID |
| `products` | — | `[Product!]!` | List all products |
| `product` | `id: String!` | `Product!` | Get a product by ID |

#### Mutations

| Mutation | Input | Returns | Description |
|---|---|---|---|
| `createUser` | `user: UserInput!` | `User!` | Create a new user |
| `createProduct` | `product: ProductInput!` | `Product!` | Create a new product |

**UserInput** — `{ name: String!, email: String! }`

**ProductInput** — `{ userId: String!, name: String!, price: Float! }`

---

# Example Queries

### List all users and products

```graphql
{
  users {
    id
    name
    email
  }
  products {
    id
    name
    price
  }
}
```

### Get a single user

```graphql
{
  user(id: "u1") {
    name
    email
  }
}
```

### Get a product with its owning user (nested)

```graphql
{
  product(id: "p1") {
    name
    price
    user {
      name
      email
    }
  }
}
```

### Create a user

```graphql
mutation {
  createUser(user: { name: "Charlie", email: "charlie@example.com" }) {
    id
    name
    email
  }
}
```

### Create a product

```graphql
mutation {
  createProduct(product: { userId: "u1", name: "Thingamajig", price: 19.99 }) {
    id
    name
    price
    user {
      name
    }
  }
}
```