-- magna quickstart seed schema.
-- Runs as postgres on first container boot via /docker-entrypoint-initdb.d.

create extension if not exists "pgcrypto";

create table users (
    id         uuid primary key default gen_random_uuid(),
    email      text not null unique,
    created_at timestamptz not null default now()
);

create table todos (
    id         uuid primary key default gen_random_uuid(),
    user_id    uuid not null references users(id) on delete cascade,
    title      text not null,
    done       boolean not null default false,
    due_date   date,
    created_at timestamptz not null default now()
);

create index todos_user_id_idx on todos(user_id);
create index todos_done_idx    on todos(done);

create table tags (
    id   uuid primary key default gen_random_uuid(),
    name text not null unique
);

create table todo_tags (
    todo_id uuid not null references todos(id) on delete cascade,
    tag_id  uuid not null references tags(id)  on delete cascade,
    primary key (todo_id, tag_id)
);

create index todo_tags_tag_id_idx on todo_tags(tag_id);

-- Deterministic UUIDs so the README examples copy-paste cleanly.
insert into users (id, email) values
    ('11111111-1111-1111-1111-111111111111', 'ada@example.com'),
    ('22222222-2222-2222-2222-222222222222', 'grace@example.com');

insert into tags (id, name) values
    ('aaaaaaaa-0000-0000-0000-00000000000a', 'work'),
    ('aaaaaaaa-0000-0000-0000-00000000000b', 'home'),
    ('aaaaaaaa-0000-0000-0000-00000000000c', 'urgent');

insert into todos (id, user_id, title, done, due_date) values
    ('aaaaaaa1-0000-0000-0000-000000000001', '11111111-1111-1111-1111-111111111111', 'write quickstart',         true,  '2026-04-20'),
    ('aaaaaaa1-0000-0000-0000-000000000002', '11111111-1111-1111-1111-111111111111', 'review PR #42',            false, '2026-04-28'),
    ('aaaaaaa1-0000-0000-0000-000000000003', '11111111-1111-1111-1111-111111111111', 'pay rent',                 false, '2026-05-01'),
    ('aaaaaaa1-0000-0000-0000-000000000004', '11111111-1111-1111-1111-111111111111', 'call dentist',             false, null),
    ('aaaaaaa1-0000-0000-0000-000000000005', '22222222-2222-2222-2222-222222222222', 'finish compiler chapter',  false, '2026-04-30'),
    ('aaaaaaa1-0000-0000-0000-000000000006', '22222222-2222-2222-2222-222222222222', 'reply to thesis advisor',  true,  '2026-04-15'),
    ('aaaaaaa1-0000-0000-0000-000000000007', '22222222-2222-2222-2222-222222222222', 'groceries',                false, '2026-04-26');

insert into todo_tags (todo_id, tag_id) values
    ('aaaaaaa1-0000-0000-0000-000000000001', 'aaaaaaaa-0000-0000-0000-00000000000a'),
    ('aaaaaaa1-0000-0000-0000-000000000002', 'aaaaaaaa-0000-0000-0000-00000000000a'),
    ('aaaaaaa1-0000-0000-0000-000000000002', 'aaaaaaaa-0000-0000-0000-00000000000c'),
    ('aaaaaaa1-0000-0000-0000-000000000003', 'aaaaaaaa-0000-0000-0000-00000000000b'),
    ('aaaaaaa1-0000-0000-0000-000000000003', 'aaaaaaaa-0000-0000-0000-00000000000c'),
    ('aaaaaaa1-0000-0000-0000-000000000005', 'aaaaaaaa-0000-0000-0000-00000000000a'),
    ('aaaaaaa1-0000-0000-0000-000000000007', 'aaaaaaaa-0000-0000-0000-00000000000b');
