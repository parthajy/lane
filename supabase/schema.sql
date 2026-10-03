-- Lane's only database off the Mac.
--
-- The app never touches this. It holds what happens on the website: the
-- waitlist, feedback, download counts, sales, and the pool of licence keys
-- signed offline. Paste this whole file into the Supabase SQL editor.
--
-- Nothing here can be read with the publishable key. The website only calls
-- the three functions at the bottom, which return counts, never rows.

create table if not exists public.waitlist (
  id          bigint generated always as identity primary key,
  email       text        not null unique,
  name        text        not null default '',
  source      text        not null default '',
  lifetime    boolean     not null default false,
  created_at  timestamptz not null default now()
);

create table if not exists public.feedback (
  id          bigint generated always as identity primary key,
  kind        text        not null default 'idea',
  body        text        not null,
  email       text        not null default '',
  created_at  timestamptz not null default now()
);

create table if not exists public.downloads (
  id          bigint generated always as identity primary key,
  source      text        not null default '',
  created_at  timestamptz not null default now()
);

-- Keys are signed on Partha's Mac and uploaded here, so no signing secret
-- ever lives on a server. A sale claims one; claimed_at makes that a
-- one-time thing even if the thank-you page is reloaded.
create table if not exists public.licence_keys (
  id           bigint generated always as identity primary key,
  plan         text        not null check (plan in ('monthly','yearly','lifetime')),
  licence_key  text        not null unique,
  claimed_by   text,
  payment_id   text unique,
  claimed_at   timestamptz,
  created_at   timestamptz not null default now()
);
create index if not exists licence_keys_free on public.licence_keys (plan) where claimed_at is null;

create table if not exists public.sales (
  id           bigint generated always as identity primary key,
  payment_id   text unique,
  email        text        not null default '',
  plan         text        not null default '',
  amount_cents integer     not null default 0,
  currency     text        not null default 'USD',
  status       text        not null default 'paid',
  created_at   timestamptz not null default now()
);

-- Locked by default: the publishable key can reach nothing directly.
-- How many installs checked in, and nothing more.
--
-- Every running Lane asks the site whether a newer version exists when it
-- starts, which it has always done and the security page has always said.
-- Counting those requests is the only thing Lane knows about its own use.
--
-- The fingerprint is a hash of the caller's address together with the date
-- and a secret. Salting it with the date is the point: the same machine
-- hashes differently tomorrow, so this can answer "how many today" and can
-- never answer "is this the same one as last week". It is a headcount that
-- cannot grow into a profile.
create table if not exists public.update_checks (
  day         date    not null,
  target      text    not null default '',
  fingerprint text    not null,
  checks      integer not null default 1,
  primary key (day, target, fingerprint)
);
create index if not exists update_checks_day on public.update_checks (day desc);

alter table public.waitlist     enable row level security;
alter table public.feedback     enable row level security;
alter table public.downloads    enable row level security;
alter table public.licence_keys enable row level security;
alter table public.update_checks enable row level security;
alter table public.sales        enable row level security;

-- How many lifetime seats the promise covers.
create or replace function public.lane_lifetime_seats() returns integer
language sql immutable as $$ select 200 $$;

-- How many tester seats are gone.
--
-- A seat is taken by either route: somebody claims one on the site, or it is
-- given to them by hand from the admin page, which is how the first testers
-- got theirs. Counting one table missed the other, so the site read nought
-- while three people were already testing. Distinct, because somebody who
-- signs up and is then sent a licence is one tester, not two.
create or replace function public.lane_seats_taken() returns integer
language sql
security definer
set search_path = public
as $$
  select count(*)::integer from (
    select lower(email) as who from waitlist where lifetime
    union
    select lower(claimed_by) from licence_keys where plan = 'comp' and claimed_at is not null
  ) both_ways
$$;

-- Join the waitlist. Returns the position, whether a lifetime seat was held,
-- and how many are left. Running it twice with the same address changes
-- nothing and gives the same answer.
create or replace function public.join_waitlist(p_email text, p_name text default '', p_source text default '')
returns json
language plpgsql
security definer
set search_path = public
as $$
declare
  v_email    text := lower(trim(p_email));
  v_seats    integer := public.lane_lifetime_seats();
  v_taken    integer;
  v_lifetime boolean;
  v_id       bigint;
  v_position integer;
begin
  if v_email !~ '^[^@\s]+@[^@\s]+\.[^@\s]+$' or length(v_email) > 160 then
    raise exception 'that does not look like an email address';
  end if;

  v_taken := public.lane_seats_taken();
  insert into waitlist (email, name, source, lifetime)
  values (v_email, left(coalesce(p_name, ''), 80), left(coalesce(p_source, ''), 40), v_taken < v_seats)
  on conflict (email) do nothing;

  select lifetime, id into v_lifetime, v_id from waitlist where email = v_email;
  select count(*) into v_position from waitlist where id <= v_id;
  v_taken := public.lane_seats_taken();

  return json_build_object(
    'ok', true,
    'position', v_position,
    'lifetime', v_lifetime,
    'lifetimeLeft', greatest(0, v_seats - v_taken),
    'lifetimeSeats', v_seats
  );
end $$;

-- What the counter on the site reads. Counts only, never addresses.
create or replace function public.waitlist_stats() returns json
language sql
security definer
set search_path = public
as $$
  select json_build_object(
    'total', (select count(*) from waitlist),
    'lifetimeLeft', greatest(0, public.lane_lifetime_seats() - public.lane_seats_taken()),
    'lifetimeSeats', public.lane_lifetime_seats()
  )
$$;

-- Feedback from the website.
create or replace function public.send_feedback(p_kind text, p_body text, p_email text default '')
returns json
language plpgsql
security definer
set search_path = public
as $$
begin
  if length(trim(coalesce(p_body, ''))) < 4 then
    raise exception 'say a little more';
  end if;
  insert into feedback (kind, body, email)
  values (left(coalesce(p_kind, 'idea'), 20), left(p_body, 4000), left(coalesce(p_email, ''), 160));
  return json_build_object('ok', true);
end $$;

revoke all on function public.lane_seats_taken() from public;
revoke all on function public.join_waitlist(text, text, text) from public;
revoke all on function public.waitlist_stats() from public;
revoke all on function public.send_feedback(text, text, text) from public;
grant execute on function public.join_waitlist(text, text, text) to anon, authenticated;
grant execute on function public.waitlist_stats() to anon, authenticated;
grant execute on function public.send_feedback(text, text, text) to anon, authenticated;

-- A sale claims one key from the pool. `for update skip locked` means two
-- buyers at the same instant can never be handed the same key, and a
-- reloaded thank-you page gets back the key that payment already claimed.
create or replace function public.claim_licence(
  p_payment_id text,
  p_plan       text,
  p_email      text default '',
  p_amount     integer default 0,
  p_currency   text default 'USD'
) returns json
language plpgsql
security definer
set search_path = public
as $$
declare
  v_key text;
  v_id  bigint;
begin
  select licence_key into v_key from licence_keys where payment_id = p_payment_id;
  if v_key is not null then
    return json_build_object('ok', true, 'key', v_key, 'plan', p_plan, 'again', true);
  end if;

  select id into v_id
    from licence_keys
   where plan = p_plan and claimed_at is null
   order by id
   limit 1
     for update skip locked;

  if v_id is null then
    return json_build_object('ok', false, 'error', 'no keys left for that plan');
  end if;

  update licence_keys
     set claimed_by = lower(coalesce(p_email, '')), payment_id = p_payment_id, claimed_at = now()
   where id = v_id
   returning licence_key into v_key;

  insert into sales (payment_id, email, plan, amount_cents, currency)
  values (p_payment_id, lower(coalesce(p_email, '')), p_plan, coalesce(p_amount, 0), coalesce(p_currency, 'USD'))
  on conflict (payment_id) do nothing;

  return json_build_object('ok', true, 'key', v_key, 'plan', p_plan);
end $$;

-- Only the server may claim: never the website, never the publishable key.
revoke all on function public.claim_licence(text, text, text, integer, text) from public, anon, authenticated;
grant execute on function public.claim_licence(text, text, text, integer, text) to service_role;

-- One check-in, counted. A repeat from the same machine on the same day
-- bumps the count rather than adding a row, so installs and checks are both
-- answerable from the one table.
create or replace function public.note_update_check(p_target text, p_fingerprint text)
returns void language sql security definer set search_path = public as $fn$
  insert into update_checks (day, target, fingerprint)
  values (current_date, left(coalesce(p_target, ''), 40), left(p_fingerprint, 64))
  on conflict (day, target, fingerprint) do update set checks = update_checks.checks + 1
$fn$;

-- What the admin page reads. Counts only, never a fingerprint.
create or replace function public.update_check_stats()
returns json language sql security definer set search_path = public as $fn$
  select json_build_object(
    'today',     (select count(*) from update_checks where day = current_date),
    'yesterday', (select count(*) from update_checks where day = current_date - 1),
    'week',      (select count(distinct fingerprint) from update_checks where day > current_date - 7),
    'days',      (select coalesce(json_agg(d order by d->>'day' desc), '[]'::json)
                  from (select json_build_object('day', day, 'installs', count(*), 'checks', sum(checks)) as d
                        from update_checks where day > current_date - 14 group by day) x)
  )
$fn$;

revoke all on function public.note_update_check(text, text) from public;
revoke all on function public.update_check_stats() from public;
