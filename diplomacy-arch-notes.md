# Diplomacy TUI — Architecture Notes

> Exported conversation covering the full system architecture, how every file connects, the network thread design, Supabase migration, snapshot/queue system, and the complete file map.

---

## Table of Contents

1. [What You Have Today](#1-what-you-have-today)
2. [How the Code Works](#2-how-the-code-works)
3. [The TUI and Map Server](#3-the-tui-and-map-server)
4. [What's Missing — The Network Thread](#4-whats-missing--the-network-thread)
5. [Target Architecture — 3 Threads, 1 Channel](#5-target-architecture--3-threads-1-channel)
6. [How Commands Integrate with the TUI](#6-how-commands-integrate-with-the-tui)
7. [Concrete Example: Player Joins](#7-concrete-example-player-joins)
8. [Supabase Migration — Data Ownership](#8-supabase-migration--data-ownership)
9. [Snapshot Lifecycle](#9-snapshot-lifecycle)
10. [Offline Order Queue](#10-offline-order-queue)
11. [Supabase Schema](#11-supabase-schema)
12. [Tokens and Auth — What Stays](#12-tokens-and-auth--what-stays)
13. [Complete File Map](#13-complete-file-map)
14. [File Dependency Graph](#14-file-dependency-graph)

---

## 1. What You Have Today

The codebase splits into 4 subsystems:

### Auth & Sessions (`session.rs`)

Handles persisting a login token to disk. `SessionKeeper` is a trait so you can mock it in tests. `FileSessionKeeper` reads/writes a UUID to `~/.config/session.json`. The `MockSessionKeeper` (via `mockall`) lets you test commands without touching the filesystem.

### Commands (`login.rs`, `register.rs`, `create.rs`, `join.rs`, `order.rs`, `map.rs`)

All implement a `Command` trait with `async fn execute()`. Each follows the same pattern:

1. Load or create a session token
2. Format a semicolon-delimited message (e.g. `LOGIN;username;password\n`)
3. Send it over a `Client` trait (network abstraction)
4. Read back a response and parse it

They're generic over `C: Client` and `S: SessionKeeper` — you can swap real TCP for a mock in tests.

### Interactive Order Entry (`state_machine.rs`, `order_builder.rs`, `util.rs`, `fake_context.rs`)

The TUI state machine for entering Diplomacy orders:

- `StateMachine` holds a `UiState` enum (show units → pick move → confirm → show orders → terminal)
- Each state implements `render()`, `handle_input()`, and `next()`
- `MachineData` carries working state (selected unit, draft order, accumulated orders)
- `OrderBuilder` is a builder pattern constructing `MappedMainOrder` from unit + command
- The `order.rs` command either parses orders from flags OR falls back to this interactive state machine

### Data Layer (`connection_pool.rs`, `user.rs`, `game.rs`, `mod.rs`)

**Server-side** persistence using SeaORM:

- `ConnectionPool` wraps a Postgres connection via `Arc<DatabaseConnection>`
- `user.rs` and `game.rs` are SeaORM entity models mapping to `users` and `games` tables
- `add_user` hashes a password and inserts a row
- This code lives on the **host**, not the client

### Host-Side Game Logic (`order_service.rs`, `order_collector.rs`, `order_repository.rs`)

- `OrderService` routes orders to `GameHandler` via a global `GAME_REGISTRY`
- `OrderCollector` trait with `MainOrderCollector`, `RetreatOrderCollector`, `BuildOrderCollector`
- Each collector validates orders (correct phase, correct unit count, correct positions) and buffers per-player
- `OrderRepository` is a stub wrapping `ConnectionPool`

---

## 2. How the Code Works

### Current connection flow

```
Client binary starts
  → User runs a command (login, register, create, join, order)
  → Command uses Client trait to send TCP message to host
  → Host processes it (auth against DB, game logic)
  → Response comes back
  → Client parses response
```

### The state machine for orders

Only kicks in during the `order` command as a fallback when no flags are passed. Runs synchronously:

```rust
while !machine.is_finished() {
    machine.state.render(&machine.data);
    machine.update("");
}
```

### The protocol

All messages are semicolon-delimited, newline-terminated:

| Command | Format |
|---------|--------|
| Login | `LOGIN;{username};{password}\n` |
| Register | `REGISTER;{username};{password}\n` |
| Create | `CREATE;{session_id}\n` |
| Join | `JOIN;{session_id};{game_id}\n` |
| Order | `ORDER;{type};{session_id};{orders_json}\n` |
| Context | `CONTEXT;{session_id}\n` |

---

## 3. The TUI and Map Server

### The full system: 4 processes, 3 connections

```
                    SUPABASE CLOUD
              ┌────────────────────┐
              │ Auth, users,       │
              │ game registry      │
              └─────────┬──────────┘
                        │ HTTPS
     ┌──────────────────┴──────────────────┐
     │                                     │

HOST MACHINE                         CLIENT MACHINE
┌────────────────┐               ┌──────────────────────┐
│ Game server    │               │ Node.js map server   │
│ TCP :7878      │               │ server.js  TCP :7777 │
│ Validator      │               ├──────────────────────┤
│ SeaORM+Postgres│               │ Rust TUI (main.rs)   │
└────────────────┘               │  ├─ MapClient        │
         │                       │  ├─ Commands         │
         │  TCP :7878 (game)     │  ├─ App state        │
         └───────────────────────│  └─ SessionKeeper    │
                                 └──────────────────────┘
```

### How main.rs works right now

1. `main()` spawns Node child process (`server.js`) on port 7777
2. Retries `MapClient::connect()` up to 20 times until server is ready
3. Enters single-threaded loop: poll events (30ms) → draw → `map_client.poll()`
4. Tab toggles focus: Terminal mode (type commands) vs Map mode (pan/zoom)
5. **No network thread** — commands are not connected to the game host yet

### The main loop (single thread, ~30ms tick)

```
┌─ Check resize ─── send_resize() if changed
│
├─ terminal.draw() ─── Renders all widgets
│
├─ event::poll(30ms) ─── Keyboard / mouse
│    │
│    ├─ if Focus::Map → Forward to MapClient (send_key arrow/zoom bytes)
│    │
│    └─ if Focus::Terminal → execute_command() (help, status, own, quit)
│
├─ map_client.poll() ─── Non-blocking TCP read
│
└─ loop back ───────────────────────────────────┘
```

### Inside `map_client.poll()`

1. **TCP read** — non-blocking `stream.read()`
2. **Find last `\x1b[H`** — latest complete frame
3. **`parse_ansi_line()`** — parse xterm-256 colors into `Vec<AnsiCell>` per line
4. **Update `frame_lines` + `status_line`** — next `draw()` picks up new data via `AnsiWidget`

`AnsiWidget` reads `frame_lines` and writes directly into ratatui's `Buffer`. Each `AnsiCell` carries `ch` + `fg` + `bg` from xterm-256 color parsing.

---

## 4. What's Missing — The Network Thread

Right now commands are **one-shot**: send a message, get a response, done. There's no persistent connection that listens for server-pushed events (like "player joined" or "phase changed").

`listener.rs` (currently empty) is meant to become this.

You need:

### 1. An `AppEvent` enum — the event bus

```rust
pub enum AppEvent {
    PlayerJoined(String),
    PlayerLeft(String),
    PhaseChanged(String),
    OrdersReceived,
    GameState(GameContext),
    KeyInput(KeyEvent),
}
```

### 2. A network listener thread

Runs in a spawned thread, reads from the TCP connection, parses server messages, sends `AppEvent`s through an `mpsc::channel`:

```rust
fn network_listener(client: TcpStream, tx: Sender<AppEvent>) {
    // read loop → parse → tx.send(event)
}
```

### 3. TUI main loop drains the channel

```rust
loop {
    while let Ok(event) = rx.try_recv() {
        app.handle_event(event);
    }
    terminal.draw(|f| { /* render from app state */ })?;
}
```

---

## 5. Target Architecture — 3 Threads, 1 Channel

```
NETWORK THREAD              MAIN THREAD (TUI)           NODE CHILD PROCESS
                            
TCP connect                 Spawn threads               server.js
to host :7878               node + network + self        SVG → Braille render
    │                           │                            │
    ▼                           ▼                            ▼
Read loop                   rx.try_recv()               TCP :7777
Blocking read on socket     Drain all events            ANSI frames out
    │                           │                            │
    ▼                           ▼                            ▼
Parse message               app.handle_event()          Keystroke in
"PLAYER_JOINED;Bob"         Update App state            Pan / zoom / own
    │                           │
    ▼                           ▼
tx.send(AppEvent)           map_client.poll()
Into mpsc channel           Non-blocking as today
    │         ══════════►       │
    │          mpsc             ▼
    │                       terminal.draw()
    │                       Render everything
    │                           │
    ▼                           ▼
loop back                   event::poll(30ms)
                            Keyboard input
                                │
                            loop back
```

### The AppEvent enum — your event bus

```rust
pub enum AppEvent {
    // From network thread
    PlayerJoined(String),
    PlayerLeft(String),
    PhaseChanged(String),
    OrdersReceived,
    GameState(GameContext),
    GameStarted,
    Disconnected,
    Error(String),
}
```

### Key rules

- Network thread **ONLY** writes to channel — never touches App or UI
- Main thread **ONLY** reads from channel — owns all mutable state
- MapClient stays in main thread (non-blocking poll, unchanged)

### What changes in main.rs

1. Add `let (tx, rx) = mpsc::channel()` — spawn network thread with `tx`
2. Add `while let Ok(e) = rx.try_recv() { handle }` before draw in the loop

---

## 6. How Commands Integrate with the TUI

### Today (broken)

```
cargo run -- login alice pw
  → LoginCommand.execute()
  → client.send() → client.read()
  → session.save(token)
  → Process exits
```

### Target (integrated)

```
TUI terminal: "login alice pw"
  → App dispatches to network
  → Network thread sends TCP
  → Response → tx.send(event)
  → TUI updates, stays running
```

### State machine becomes a UiState

```
Lobby → Map → Orders → Results → (next turn) → Lobby → ...
```

Your existing `StateMachine` (show_units → pick_move → confirm) lives inside the **Orders** state.

### Concrete changes to main.rs

1. Add `enum AppEvent` and `enum AppCommand` (outbound requests)
2. Create `(tx, rx) = mpsc::channel()` and `(cmd_tx, cmd_rx)` for bidirectional
3. Spawn thread: `network_listener(game_stream, tx, cmd_rx)`
4. In `execute_command`: `"login"` → `cmd_tx.send(AppCommand::Login{...})`
5. In loop: drain `rx` → update `App.players`, `App.phase`, etc → draw

---

## 7. Concrete Example: Player Joins

Here's the full sequence when "Bob joins the game":

```
t=0     HOST: Bob sends JOIN
        HOST: Validates token
        HOST: Broadcasts to all → "PLAYER_JOINED;Bob\n"
                    │
                    │ TCP
                    ▼
~1ms    NETWORK THREAD: socket.read() wakes
        NETWORK THREAD: Parse protocol "PLAYER_JOINED;Bob"
        NETWORK THREAD: tx.send(AppEvent::PlayerJoined("Bob"))
                    │
                    │ mpsc channel
                    ▼
~2ms    TUI MAIN THREAD: rx.try_recv() → Ok
        TUI MAIN THREAD: handle_event() → players.push("Bob")

next    TUI MAIN THREAD: terminal.draw() → Bob appears in list
tick
```

### The key rule

- Network thread never writes to App. It only puts events in the channel.
- Main thread is the only thing that mutates state. No `Arc<Mutex>` needed.

---

## 8. Supabase Migration — Data Ownership

### Today

Host owns ALL data:

- `ConnectionPool` → local Postgres
- `users` table (SeaORM)
- `games` table (SeaORM)
- Game engine (live state in memory)
- Order resolver (validation + resolution)

**Problem**: host dies = all data lost. Host offline = can't submit orders.

### Target

Split ownership:

**Supabase (source of truth)**:
- `users` — auth + profile
- `games` — registry + metadata
- `snapshots` — full game state as JSON
- `order_queue` — pending orders buffer
- `game_players`, `turn_history`, `results`

**Host (temporary authority)**:
- Live state — in memory only
- Resolver — validate + run

**The rule**: Supabase = durable state. Host = temporary compute.

### ConnectionPool migration is one line

```
postgres://local:5432/diplo → postgres://user:pw@db.supabase.co:5432/postgres
```

Your SeaORM models (`user.rs`, `game.rs`) stay identical. `ConnectionPool` struct stays as-is. `add_user` function stays as-is.

---

## 9. Snapshot Lifecycle

### What a snapshot contains (JSON)

```json
{
  "game_id": "...",
  "turn": 5,
  "phase": "SpringMovement",
  "units": { "FRA": [["Army", "par"], ["Fleet", "bre"]], ... },
  "owners": { "par": "FRA", "ber": "GER", ... },
  "occupiers": { "par": "FRA", "mun": "GER", ... }
}
```

This is your `GameContext` serialized.

### The flow

```
Host starts (cargo run host)
  → Pull latest snapshot: SELECT FROM snapshots WHERE game_id = ? ORDER BY turn DESC LIMIT 1
  → Hydrate game: JSON → GameContext
  → Game runs normally (orders, resolution, turns)
  → Turn resolves
  → Push snapshot to Supabase: INSERT INTO snapshots (game state JSON)
  → Next turn (loop)
```

---

## 10. Offline Order Queue

### When host is offline

```
CLIENT                     SUPABASE                  HOST

                                                     Host goes offline

Submit order ──────────►  Queue order
                          status = "pending"

Submit order 2 ─────────► Queue order
                          status = "pending"

                                                     Host reconnects
                                                         │
                          Pull pending ◄─────────────────┘
                          WHERE status=pending
                                                         │
                                                     Process + resolve
                                                     Mark status = "resolved"
                                                         │
                          Update + new snapshot ◄────────┘
```

### The offline queue contract

- Client tries host first (direct TCP). If unreachable → POST to Supabase queue.
- Host on startup: pull latest snapshot + pull all pending orders.
- Host after each turn: push snapshot + mark orders resolved.
- Host periodically: poll queue for new pending orders (every ~30s or so).

---

## 11. Supabase Schema

### `users` table (already exists)

| Column | Type |
|--------|------|
| `user_id` | uuid PK |
| `username` | text |
| `password_hash` | text |
| `created_at` | timestamp |

### `games` table (already exists, add columns)

| Column | Type |
|--------|------|
| `game_id` | uuid PK |
| `name` | text |
| `host_user_id` | uuid FK → users |
| `host_ip` | text |
| `host_port` | int |
| `join_code` | text |
| `status` | text (open / active / paused / finished) |
| `created_at` | timestamp |

### `game_players` table (new)

| Column | Type |
|--------|------|
| `id` | uuid PK |
| `game_id` | uuid FK → games |
| `user_id` | uuid FK → users |
| `nation` | text |
| `joined_at` | timestamp |

### `snapshots` table (new)

| Column | Type |
|--------|------|
| `id` | uuid PK |
| `game_id` | uuid FK → games |
| `turn_number` | int |
| `phase` | text |
| `game_state` | jsonb (full GameContext) |
| `created_at` | timestamp |

### `order_queue` table (new)

| Column | Type |
|--------|------|
| `id` | uuid PK |
| `game_id` | uuid FK → games |
| `user_id` | uuid FK → users |
| `turn_number` | int |
| `phase` | text |
| `orders` | jsonb (Vec of orders) |
| `status` | text (pending / resolved) |
| `created_at` | timestamp |

### `turn_history` table (new, optional)

For replay/audit. Stores resolved orders + results per turn.

---

## 12. Tokens and Auth — What Stays

Your `SessionKeeper` and token system stays **exactly as it is**.

`FileSessionKeeper` saves a UUID to `~/.config/session.json` on the client machine. That's a *game session* token — it identifies you to the host when you send commands like `JOIN;{token};{game_id}\n`. This is separate from Supabase auth.

You end up with **two tokens**:

| Token | Purpose | Where stored | Used for |
|-------|---------|-------------|----------|
| **Supabase JWT** | Proves identity | Client disk (e.g. `supabase_token.json`) | HTTPS calls to Supabase (reading snapshots, writing to order queue when host offline) |
| **Game session UUID** | Identifies you to host | `~/.config/session.json` via `SessionKeeper` | Every TCP message to the host |

The flow becomes:

1. Client authenticates with Supabase first (gets JWT)
2. Connects to host over TCP and presents that JWT
3. Host verifies JWT against Supabase
4. If valid, issues the game session UUID back

Your existing `RegisterCommand` / `LoginCommand` format (`REGISTER;username;password\n`) could evolve to `AUTH;{jwt}\n`, but that's optional. Either way, `SessionKeeper`, `FileSessionKeeper`, and `MockSessionKeeper` all stay as they are.

---

## 13. Complete File Map

### Battle TUI (client binary) — Rust, ratatui + crossterm

| Status | File | Description |
|--------|------|-------------|
| **CHANGED** | `main.rs` | Add mpsc channel, spawn network thread, drain events in loop. MapClient stays. |
| EXISTS | `map_client.rs` | TCP to Node map server. AnsiWidget + poll(). No changes needed. |
| **FILL STUB** | `listener.rs` | → Becomes the network thread. Blocking TCP read loop, parses host messages, tx.send(AppEvent). |
| **NEW** | `events.rs` | `enum AppEvent { PlayerJoined, PhaseChanged, GameState, ... }` + `enum AppCommand { Login, Order, ... }` |
| **NEW** | `app.rs` | App struct (players, phase, map_client, focus). `handle_event()` mutates state from events. |
| **NEW** | `supabase_client.rs` | HTTPS fallback for order queue when host offline. Uses Supabase JWT. |

### Commands (client-side) — Rust, async_trait

| Status | File | Description |
|--------|------|-------------|
| EXISTS | `util.rs` | Command trait, Client trait, CommandError enum. No changes. |
| EXISTS | `login.rs` | LoginCommand — sends `LOGIN;user;pw`, saves session token. No changes. |
| EXISTS | `register.rs` | RegisterCommand — sends `REGISTER;user;pw`, saves session token. No changes. |
| EXISTS | `create.rs` | CreateCommand — sends `CREATE;session_id`. No changes. |
| EXISTS | `join.rs` | JoinCommand — sends `JOIN;session_id;game_id`. No changes. |
| EXISTS | `order.rs` | OrderCommand — parse flags or interactive fallback. No changes. |
| EXISTS | `map.rs` | MapCommand — stub. No changes. |
| EXISTS | `connect.rs` | ConnectCommand — commented out. No changes. |

### Auth + Interactive (client-side) — Rust

| Status | File | Description |
|--------|------|-------------|
| EXISTS | `session.rs` | SessionKeeper trait, FileSessionKeeper, MockSessionKeeper. No changes. |
| EXISTS | `state_machine.rs` | UiState enum, StateMachine, MachineData, OrderDraft. No changes. |
| EXISTS | `order_builder.rs` | Builder pattern for MappedMainOrder. No changes. |
| EXISTS | `util.rs (interactive)` | select_from() helper, UnitAt wrapper, finalize_order(). No changes. |
| EXISTS | `fake_context.rs` | Test fixture. Hardcoded France game state. Replaced by real snapshots in production. |

### Host server — Rust, tokio

| Status | File | Description |
|--------|------|-------------|
| EXISTS | `order_service.rs` | Routes orders to GameHandler via GAME_REGISTRY. No changes. |
| EXISTS | `order_collector.rs` | Main/Retreat/Build collectors. Validates + buffers per player. No changes. |
| EXISTS | `order_repository.rs` | Stub — wraps ConnectionPool. No changes. |
| **NEW** | `snapshot_service.rs` | Pull snapshot on boot, push after each turn. Supabase via ConnectionPool. |
| **NEW** | `queue_poller.rs` | Background task: poll order_queue every ~30s, feed to resolver. |

### Map server — Node.js

| Status | File | Description |
|--------|------|-------------|
| EXISTS | `server.js` | TCP server, Viewport, MapRenderer, ANSI output. No changes. |
| EXISTS | `svg-loader.js` | SVG parser → polylines, fills, strokes, labels. No changes. |
| EXISTS | `config.js` | useBraille, delimiter. No changes. |
| EXISTS | `utils.js` | population() bit counter. No changes. |

### Data layer — Rust, SeaORM → Supabase Postgres

| Status | File | Description |
|--------|------|-------------|
| **CHANGED** | `connection_pool.rs` | Connection string changes to Supabase URL. Struct stays identical. |
| EXISTS | `user.rs` | SeaORM entity: user_id, username, password_hash, created_at. No changes. |
| EXISTS | `game.rs` | SeaORM entity: game_id, name, year, game_phase. No changes. |
| **CHANGED** | `mod.rs (data)` | Add `pub mod snapshot; pub mod order_queue;` |
| **NEW** | `snapshot.rs` | SeaORM entity: id, game_id, turn_number, phase, game_state (jsonb), created_at. |
| **NEW** | `order_queue.rs` | SeaORM entity: id, game_id, user_id, turn, phase, orders (jsonb), status, created_at. |

---

## 14. File Dependency Graph

```
main.rs
  ├──► listener.rs (network thread)
  │       ├──► events.rs (AppEvent + AppCommand)
  │       └──► commands/* (login, register, create, join, order)
  │               └──► session.rs (SessionKeeper)
  │
  ├──► app.rs (App state + TUI)
  │       ├──► events.rs (reads AppEvent)
  │       ├──► state_machine.rs (interactive order entry)
  │       │       ├──► order_builder.rs
  │       │       ├──► util.rs (interactive)
  │       │       └──► fake_context.rs (test only)
  │       └──► supabase_client.rs (order queue fallback)
  │
  └──► map_client.rs (TCP to Node server)
          └──► server.js (:7777)
                  ├──► svg-loader.js
                  ├──► config.js
                  └──► utils.js

HOST SIDE:
  order_service.rs
      └──► order_collector.rs (Main/Retreat/Build)
  order_repository.rs
      └──► connection_pool.rs
  snapshot_service.rs (NEW)
      └──► connection_pool.rs → snapshot.rs (SeaORM)
  queue_poller.rs (NEW)
      └──► connection_pool.rs → order_queue.rs (SeaORM)

DATA LAYER:
  connection_pool.rs → Supabase Postgres
      ├──► user.rs (SeaORM entity)
      ├──► game.rs (SeaORM entity)
      ├──► snapshot.rs (SeaORM entity, NEW)
      └──► order_queue.rs (SeaORM entity, NEW)
```

---

## Summary

- **~25 existing files** — stay untouched
- **6 new files** to create: `events.rs`, `app.rs`, `supabase_client.rs`, `snapshot_service.rs`, `queue_poller.rs`, `snapshot.rs` + `order_queue.rs` (data models)
- **1 empty stub** to fill: `listener.rs`
- **3 files** need small changes: `main.rs` (add channel + spawn network thread), `connection_pool.rs` (change connection string), `data/mod.rs` (add module declarations)
- **SessionKeeper** and the game session UUID token system — **no changes**
- **MapClient** and the Node.js map server — **no changes**
- **All commands** — **no changes to logic**
- **Order collectors** on the host — **no changes**
- Supabase is **just Postgres** — SeaORM works identically, only the connection string changes
