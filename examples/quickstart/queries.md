# Sample queries

These run against the schema in `seed.sql`. Paste any of them into the
GraphiQL UI at `http://localhost:4800/graphql`, or wrap them in a curl
`-d '{"query":"..."}'` body. magna's auto-CRUD follows the
relay-connection convention: list fields return `{ nodes, totalCount,
pageInfo }`, single-row fetchers are named `<type>ById`, and mutations
return a payload object containing the affected row.

UUIDs below match the deterministic ids inserted by `seed.sql`.

## 1. All users

```graphql
{
  allUsers {
    nodes { id email createdAt }
    totalCount
  }
}
```

## 2. Todos for one user

```graphql
{
  allTodos(filter: { userId: { equalTo: "11111111-1111-1111-1111-111111111111" } }) {
    nodes { id title done dueDate }
  }
}
```

## 3. Open todos only

```graphql
{
  allTodos(filter: { done: { equalTo: false } }) {
    totalCount
    nodes { id title userId }
  }
}
```

## 4. Order by due date

```graphql
{
  allTodos(orderBy: [DUE_DATE_ASC, ID_ASC]) {
    nodes { title dueDate done }
  }
}
```

## 5. Paginate

```graphql
{
  allTodos(first: 3, offset: 0, orderBy: CREATED_AT_DESC) {
    nodes { id title }
    pageInfo { hasNextPage endCursor }
    totalCount
  }
}
```

Then page two:

```graphql
{
  allTodos(first: 3, offset: 3, orderBy: CREATED_AT_DESC) {
    nodes { id title }
  }
}
```

## 6. Single row by id

```graphql
{
  todoById(id: "aaaaaaa1-0000-0000-0000-000000000002") {
    id
    title
    done
    user { email }
  }
}
```

## 7. Create a todo

```graphql
mutation {
  createTodo(input: {
    todo: {
      userId: "22222222-2222-2222-2222-222222222222"
      title: "ship magna 0.1"
      dueDate: "2026-05-15"
    }
  }) {
    todo { id title done dueDate }
  }
}
```

## 8. Update a todo's done flag

```graphql
mutation {
  updateTodoById(input: {
    id: "aaaaaaa1-0000-0000-0000-000000000003"
    patch: { done: true }
  }) {
    todo { id title done }
  }
}
```

## 9. Delete a todo

```graphql
mutation {
  deleteTodoById(input: { id: "aaaaaaa1-0000-0000-0000-000000000004" }) {
    deletedTodoId
  }
}
```

## 10. Joined: user with todos and each todo's tags

magna walks foreign keys both ways, so the reverse relation `todos` and
the join through `todo_tags` to `tags` are both auto-generated.

```graphql
{
  userById(id: "11111111-1111-1111-1111-111111111111") {
    email
    todos(orderBy: DUE_DATE_ASC) {
      nodes {
        title
        done
        dueDate
        todoTags {
          nodes {
            tag { name }
          }
        }
      }
    }
  }
}
```
