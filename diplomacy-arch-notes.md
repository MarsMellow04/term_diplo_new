# Diplomacy TUI — Simplified Architecture & UML Reference

This is a planning document, not code. It exists to answer one question before you touch `cargo new`: **what's the smallest set of components that still gives you every feature you asked for, expressed with real GoF patterns and real Rust idioms, without regrowing into the 25-file sprawl from last time?**

Everything below assumes the feature list from your prompt:
- Host sets up a game → `game_id` → shares it
- Accounts, registration, login
- Players join a game → feed updates for everyone connected
- Moves can be submitted when the host is offline (queued)
- Messaging
- Order entry via a finite-state-machine UI (you called it "Turing machine-like" — see the terminology note below)
- Game browser shows only *discoverable* games
- Back navigation from any screen
- GoF-style OOP, Rust idioms

---

## 0. Terminology note

A "Turing machine-like UI" — a machine with a fixed set of states, one active state at a time, and transitions triggered by input — is a **finite state machine (FSM)**, which is exactly what the GoF **State pattern** formalizes as OOP. You already had this instinct in the old `state_machine.rs`. This doc keeps it, but promotes it from "the thing order entry does" to "the thing the *entire TUI* does" — screens are states too, which is what gives you back-navigation for free (see §7).

---

## 1. Stack

| Concern | Choice | Why |
|---|---|---|
| Language | Rust, Cargo **workspace** with 3 crates | Compiler-enforced module boundaries instead of convention |
| TUI | `ratatui` + `crossterm` | Unchanged from before, it works |
| Async runtime | `tokio` | Needed for the host's concurrent connections regardless |
| Host↔Client protocol | Raw TCP, newline-delimited **JSON** via `serde_json` | Replaces the hand-rolled `LOGIN;user;pw` parser — `#[derive(Serialize, Deserialize)]` on one `enum Protocol` eliminates a whole category of parsing bugs and is the "leverage Rust" move for the wire format |
| Backend | Supabase: **Auth** + **PostgREST** (`/rest/v1/...` over `reqwest`) + **Realtime Broadcast** | No SeaORM, no direct Postgres connection anywhere — see §2 |
| Errors | `thiserror` inside each crate, `anyhow` only at the two `main.rs` binaries | Library code returns typed errors; binaries collapse them |

**Cut for now:** the Node.js SVG→braille map renderer. It's a separate process with its own retry/lifecycle logic and it doesn't touch any of the 8 features you listed. Render the board as a `ratatui` table (territory → owner → unit) instead. Bring the fancy map back later as an optional widget once the core loop is solid — it was never coupled to game logic anyway, so nothing here forecloses it.

---

## 2. What got simplified, and why

| Old | New | Reasoning |
|---|---|---|
| SeaORM + direct Postgres pool (`connection_pool.rs`, `user.rs`, `game.rs` as entities) | Supabase REST (`PersistenceGateway` trait, one `reqwest`-based implementation) | Supabase already **is** a generated CRUD API (PostgREST). An ORM on top of an ORM-shaped API is a redundant layer. This deletes an entire subsystem. |
| Two data-access paths (host uses SeaORM, client falls back to a separate "Supabase client" for the offline queue) | **One** `PersistenceGateway` trait, used identically by host and client | Host and client were always doing the same kind of thing — read a row, write a row — they just used different tools to do it. Unify the tool. |
| Chat implied to route through the host (implicitly, since it wasn't designed) | Messaging never touches the host. Client writes a row to `messages`, other clients get it via **Supabase Realtime Broadcast** or a light poll. | A chat message has no game-rules meaning — it doesn't need adjudication, so it doesn't need the host. Decoupling it means chat still works while the host process is down, for free. |
| `state_machine.rs`, `order_builder.rs`, `util.rs`, `fake_context.rs` scattered at top level | One `screens/order_entry.rs` module containing the FSM + builder | These four files were all "how does a player build one order" — that's one responsibility, one module. |
| `order_repository.rs` (a stub wrapping `ConnectionPool`) | Deleted — folded into `SnapshotService` / `QueuePoller` | It did nothing a direct call couldn't. |
| Global `GAME_REGISTRY` (implied `static`/`lazy_static`) | `Arc<RwLock<HashMap<GameId, GameHandle>>>` created once in `main`, cloned into every connection task | Classic GoF Singleton assumes global mutable state is fine. Rust's ownership model makes that an anti-pattern (needs `unsafe` or a `OnceCell` workaround for no real benefit) — dependency injection via `Arc::clone` gives you the "one shared registry" property *and* keeps the borrow checker's guarantees. This is the one place where "GoF style" and "idiomatic Rust" actively disagree, and Rust wins. |

Net effect: **3 crates, ~20 files total**, each with one clear job, versus 25+ files where responsibility was smeared across `util.rs`-style grab bags.

---

## 3. Workspace layout

```
diplomacy/
├── Cargo.toml                    # [workspace] members = ["crates/*"]
├── crates/
│   ├── core/                     # diplomacy-core (lib, zero I/O)
│   │   ├── board.rs              #   Board, Territory, Unit
│   │   ├── order.rs              #   Order enum
│   │   ├── phase.rs              #   GamePhase enum + PhaseHandler trait [State]
│   │   ├── validator.rs          #   OrderValidator strategies       [Strategy]
│   │   └── protocol.rs           #   wire message enum, serde derive
│   │
│   ├── host/                     # diplomacy-host (bin)
│   │   ├── main.rs               #   tokio TCP listener, spawns per-connection tasks
│   │   ├── registry.rs           #   GameRegistry                    [Registry, not Singleton]
│   │   ├── game_handle.rs        #   GameHandle (one live game)
│   │   ├── order_collector.rs    #   OrderCollector strategies       [Strategy]
│   │   ├── snapshot_service.rs   #   pull/push GameContext JSON
│   │   ├── queue_poller.rs       #   pull pending orders while offline-catch-up
│   │   └── gateway.rs            #   PersistenceGateway impl (reqwest → Supabase)
│   │
│   └── client/                   # diplomacy-client (bin, the TUI)
│       ├── main.rs               #   sets up channels, spawns network thread, runs App
│       ├── app.rs                #   App: screen stack + shared game state
│       ├── screens/
│       │   ├── mod.rs            #   Screen trait + Transition enum    [State]
│       │   ├── main_menu.rs
│       │   ├── auth.rs           #   login / register forms
│       │   ├── browser.rs        #   discoverable games list
│       │   ├── lobby.rs          #   feed, players, chat entry point
│       │   ├── order_entry.rs    #   the order FSM lives here
│       │   └── messaging.rs
│       ├── commands/
│       │   ├── mod.rs            #   Command trait + CommandFactory   [Command, Factory]
│       │   ├── login.rs / register.rs / create.rs / join.rs / order.rs / message.rs
│       ├── network.rs            #   network thread, TCP Client trait impl
│       ├── events.rs             #   AppEvent, GameEvent, AppCommand
│       ├── session.rs            #   SessionKeeper — unchanged from before
│       └── gateway.rs            #   PersistenceGateway impl, same trait as host's
```

`core` has no `tokio`, no `reqwest`, no I/O of any kind — it's pure data + rules, which means it's trivially unit-testable and both `host` and `client` depend on it for shared types (`Order`, `GamePhase`, `Protocol`).

---

## 4. GoF pattern map

| Pattern | Where | Rust-specific note |
|---|---|---|
| **State** | `Screen` trait (TUI navigation) and `PhaseHandler` trait (game turn phases) | Rust enums with data make illegal states unrepresentable — `GamePhase` can't be "SpringMovement with retreat data attached." The compiler's exhaustive `match` also guarantees you never forget to handle a new phase/screen. |
| **Command** | `Command` trait + `CommandFactory` | Same structure you already had — kept as-is, it's a good fit. |
| **Strategy** | `OrderValidator` (movement/retreat/build rules differ), `OrderCollector` (same three variants, host side) | Trait objects (`Box<dyn OrderValidator>`) selected by `GamePhase` at runtime. |
| **Builder** | Order construction inside `order_entry.rs` (unit → order type → target → support target) | Each FSM step fills one field; `build()` only callable once all required fields are set — enforce with a typestate-lite pattern if you want extra safety. |
| **Observer** | `GameEvent` broadcast from host → all connected clients' feeds | Implemented as a `tokio::sync::broadcast` channel on the host and an `mpsc` channel on the client, **not** a list of observer trait objects. Channels are the idiomatic Rust Observer: the "subject" doesn't hold references to its "observers," it just clones a `Sender`. No lifetime headaches. |
| **Factory Method** | `CommandFactory::parse`, `PhaseHandler::for_phase(GamePhase)` | Centralizes "given this tag, give me the right object." |
| **Registry** (GoF calls this a variant of Singleton; see §2) | `GameRegistry` | Deliberately **not** a global static — `Arc<RwLock<HashMap<...>>>` injected via `Arc::clone`, satisfying "one shared instance" without a Rust anti-pattern. |
| **Adapter** | `PersistenceGateway` trait, single `SupabaseGateway` impl | Isolates "we talk to Supabase over REST" behind an interface, so if you ever swap backends, only `gateway.rs` changes. |
| **Template Method** | `Command::execute()` skeleton: build message → send → parse response, unchanged from your original code | Kept because it already worked. |

---

## 5. System component diagram

```mermaid
flowchart TB
    subgraph Client["Client Machine"]
        TUI["diplomacy-client\nratatui + crossterm\n(App owns Screen stack)"]
        NET["Network Thread"]
        TUI <-->|"mpsc: AppEvent / AppCommand"| NET
    end

    subgraph HostM["Host Machine"]
        HOST["diplomacy-host\ntokio TCP :7878"]
        REG["GameRegistry\nArc<RwLock<HashMap>>"]
        HOST --> REG
    end

    subgraph Supabase["Supabase Cloud"]
        AUTH["Auth"]
        REST["PostgREST /rest/v1"]
        DB[("Postgres\nusers · games · game_players\nsnapshots · order_queue · messages")]
        RT["Realtime Broadcast"]
        REST --> DB
        RT --> DB
    end

    NET <-->|"TCP :7878\ngame protocol (JSON lines)"| HOST
    TUI -->|"HTTPS: signup / login"| AUTH
    TUI -->|"HTTPS: browse games\nqueue order · post chat"| REST
    TUI -.->|"subscribe: chat + game events"| RT
    HOST -->|"HTTPS: verify JWT\nsnapshot pull/push · queue poll"| REST
    AUTH --> DB
```

---

## 6. Core domain — class diagram

```mermaid
classDiagram
    class GamePhase {
        <<enum>>
        SpringMovement
        SpringRetreat
        FallMovement
        FallRetreat
        WinterBuild
    }

    class PhaseHandler {
        <<trait>>
        +validate(order: Order, board: Board) Result~()~
        +resolve(board: Board, orders: Vec~Order~) Resolution
        +next_phase(has_dislodgements: bool) GamePhase
    }
    class SpringMovementPhase
    class RetreatPhase
    class WinterBuildPhase
    PhaseHandler <|.. SpringMovementPhase
    PhaseHandler <|.. RetreatPhase
    PhaseHandler <|.. WinterBuildPhase

    class OrderValidator {
        <<trait>>
        +validate(order: Order, board: Board) Result~()~
    }
    class MovementValidator
    class RetreatValidator
    class BuildValidator
    OrderValidator <|.. MovementValidator
    OrderValidator <|.. RetreatValidator
    OrderValidator <|.. BuildValidator
    PhaseHandler --> OrderValidator : delegates to

    class Order {
        <<enum>>
        Move(unit, target)
        Support(unit, supported, target)
        Convoy(unit, army, target)
        Hold(unit)
        Retreat(unit, target)
        Disband(unit)
        Build(power, territory, unit_type)
    }

    class Board {
        -units: HashMap~TerritoryId, Unit~
        -ownership: HashMap~TerritoryId, Power~
        +apply(order: Order)
        +adjacent(t: TerritoryId) Vec~TerritoryId~
    }

    PhaseHandler ..> Order : consumes
    PhaseHandler --> Board : mutates

    class Protocol {
        <<enum>>
        Auth(jwt: String)
        Join(game_id: Uuid)
        SubmitOrders(orders: Vec~Order~)
        SendMessage(body: String)
        Ack
        Event(GameEvent)
    }
```

---

## 7. Client — Screen subsystem (State pattern + back-stack)

This is the piece that directly answers "back navigation" and "Turing machine-like UI": `App` holds a `Vec<Box<dyn Screen>>` as a stack. Entering a new screen pushes; pressing Back pops. No separate "back logic" needed anywhere — it's a property of the stack.

```mermaid
classDiagram
    class Screen {
        <<trait>>
        +render(app: App, frame: Frame)
        +handle_input(app: App, key: KeyEvent) Transition
    }
    class MainMenuScreen
    class AuthScreen
    class GameBrowserScreen
    class LobbyScreen
    class OrderEntryScreen
    class MessagingScreen
    Screen <|.. MainMenuScreen
    Screen <|.. AuthScreen
    Screen <|.. GameBrowserScreen
    Screen <|.. LobbyScreen
    Screen <|.. OrderEntryScreen
    Screen <|.. MessagingScreen

    class Transition {
        <<enum>>
        Stay
        Push(Box~dyn Screen~)
        Pop
        Replace(Box~dyn Screen~)
    }
    Screen ..> Transition : returns

    class App {
        -screen_stack: Vec~Box~dyn Screen~~
        -game_state: Option~GameContext~
        -session: Session
        +current() Screen
        +apply(t: Transition)
        +handle_event(e: AppEvent)
    }
    App "1" *-- "many" Screen : stack = back history

    class OrderEntryScreen {
        -fsm: OrderFsm
    }
    class OrderFsm {
        <<state machine>>
        SelectUnit
        ChooseOrderType
        ChooseTarget
        ChooseSupportTarget
        ConfirmOrder
        Summary
    }
    OrderEntryScreen *-- OrderFsm
```

---

## 8. Client — Command, Networking & Persistence

```mermaid
classDiagram
    class Command {
        <<trait>>
        +execute(client: Client, session: SessionKeeper) Result~Response~
    }
    class LoginCommand
    class RegisterCommand
    class CreateGameCommand
    class JoinGameCommand
    class SubmitOrderCommand
    class SendMessageCommand
    Command <|.. LoginCommand
    Command <|.. RegisterCommand
    Command <|.. CreateGameCommand
    Command <|.. JoinGameCommand
    Command <|.. SubmitOrderCommand
    Command <|.. SendMessageCommand

    class CommandFactory {
        +parse(raw: str) Box~dyn Command~
    }
    CommandFactory ..> Command : creates

    class SessionKeeper {
        <<trait>>
        +load() Option~Session~
        +save(s: Session)
    }
    class FileSessionKeeper
    class MockSessionKeeper
    SessionKeeper <|.. FileSessionKeeper
    SessionKeeper <|.. MockSessionKeeper

    class NetClient {
        <<trait>>
        +send(msg: Protocol)
        +read() Protocol
    }
    class TcpClient
    NetClient <|.. TcpClient
    LoginCommand --> SessionKeeper : uses
    LoginCommand --> NetClient : uses

    class NetworkThread {
        -stream: TcpStream
        -tx: Sender~AppEvent~
        -cmd_rx: Receiver~AppCommand~
        +run()
    }
    class AppEvent {
        <<enum>>
        Network(GameEvent)
        Local(LocalEvent)
    }
    class GameEvent {
        <<enum>>
        PlayerJoined(String)
        PlayerLeft(String)
        PhaseChanged(GamePhase)
        OrdersResolved(Resolution)
        NewMessage(ChatMessage)
        Disconnected
        Error(String)
    }
    NetworkThread ..> AppEvent : tx.send
    AppEvent *-- GameEvent

    class PersistenceGateway {
        <<trait>>
        +fetch_discoverable_games() Vec~GameSummary~
        +queue_order(o: PendingOrder)
        +fetch_snapshot(id: Uuid) Option~Snapshot~
        +post_message(m: ChatMessage)
    }
    class SupabaseGateway
    PersistenceGateway <|.. SupabaseGateway
```

---

## 9. Host — Registry & Game subsystem

```mermaid
classDiagram
    class GameRegistry {
        -games: Arc~RwLock~HashMap~GameId, GameHandle~~~
        +get(id: GameId) Option~GameHandle~
        +create(id: GameId) GameHandle
    }
    class GameHandle {
        -context: GameContext
        -phase_handler: Box~dyn PhaseHandler~
        -broadcast: Sender~GameEvent~
        -collector: Box~dyn OrderCollector~
        +submit_order(o: Order, player: PlayerId)
        +advance_phase()
    }
    GameRegistry "1" o-- "many" GameHandle

    class OrderCollector {
        <<trait>>
        +collect(o: Order, player: PlayerId) Result~()~
        +ready() bool
    }
    class MainOrderCollector
    class RetreatOrderCollector
    class BuildOrderCollector
    OrderCollector <|.. MainOrderCollector
    OrderCollector <|.. RetreatOrderCollector
    OrderCollector <|.. BuildOrderCollector
    GameHandle *-- OrderCollector
    GameHandle --> PhaseHandler
    GameHandle ..> GameEvent : broadcasts (Observer)

    class SnapshotService {
        +pull_latest(game_id: Uuid) Snapshot
        +push(game_id: Uuid, ctx: GameContext)
    }
    class QueuePoller {
        +poll(game_id: Uuid) Vec~PendingOrder~
        +mark_resolved(ids: Vec~Uuid~)
    }
    GameHandle --> SnapshotService
    GameHandle --> QueuePoller
    SnapshotService --> PersistenceGateway
    QueuePoller --> PersistenceGateway
```

---

## 10. State diagrams

### 10a. TUI navigation (back-stack in action)

```mermaid
stateDiagram-v2
    [*] --> MainMenu
    MainMenu --> Auth: Login / Register
    Auth --> MainMenu: Back
    Auth --> GameBrowser: authenticated
    MainMenu --> GameBrowser: Browse games
    GameBrowser --> MainMenu: Back
    GameBrowser --> Lobby: Join selected game
    Lobby --> GameBrowser: Back
    Lobby --> OrderEntry: My turn to order
    OrderEntry --> Lobby: Back / Cancel
    OrderEntry --> Lobby: Orders submitted
    Lobby --> Messaging: Open chat
    Messaging --> Lobby: Back
```

### 10b. Order-entry FSM (the "Turing machine" screen)

```mermaid
stateDiagram-v2
    [*] --> SelectUnit
    SelectUnit --> ChooseOrderType: unit picked
    ChooseOrderType --> SelectUnit: Back
    ChooseOrderType --> ChooseTarget: Move / Retreat / Convoy
    ChooseOrderType --> ChooseSupportTarget: Support
    ChooseOrderType --> ConfirmOrder: Hold / Disband
    ChooseTarget --> ChooseOrderType: Back
    ChooseTarget --> ConfirmOrder: target picked
    ChooseSupportTarget --> ChooseOrderType: Back
    ChooseSupportTarget --> ConfirmOrder: support target picked
    ConfirmOrder --> ChooseOrderType: Back / edit
    ConfirmOrder --> Summary: confirmed
    Summary --> SelectUnit: edit another unit
    Summary --> [*]: Submit all orders
```

### 10c. Game phase FSM (host-side, per Diplomacy rules)

```mermaid
stateDiagram-v2
    [*] --> SpringMovement
    SpringMovement --> SpringRetreat: dislodgements exist
    SpringMovement --> FallMovement: no dislodgements
    SpringRetreat --> FallMovement
    FallMovement --> FallRetreat: dislodgements exist
    FallMovement --> WinterBuild: no dislodgements
    FallRetreat --> WinterBuild
    WinterBuild --> SpringMovement: next year
```

---

## 11. Sequence diagrams

### 11a. Register → Login

```mermaid
sequenceDiagram
    participant U as Client (TUI)
    participant SB as Supabase Auth
    participant H as Host

    U->>SB: POST /auth/v1/signup {username, password}
    SB-->>U: JWT + user_id
    U->>H: TCP connect
    U->>H: Protocol::Auth(jwt)
    H->>SB: verify JWT
    SB-->>H: valid, user_id
    H-->>U: Protocol::Ack {session_uuid}
    U->>U: SessionKeeper.save(session_uuid)
```

### 11b. Create game → discover → join → feed updates

```mermaid
sequenceDiagram
    participant Host as Host (creates game)
    participant SB as Supabase (games table)
    participant P as Player (browsing)
    participant Others as Other connected clients

    Host->>SB: INSERT games (id, host_ip, host_port, status=open, discoverable=true)
    SB-->>Host: game_id
    Note over Host: game_id shared with players out-of-band

    P->>SB: GET /games?status=eq.open&discoverable=eq.true
    SB-->>P: [GameSummary...]
    P->>Host: TCP connect host_ip:host_port
    P->>Host: Protocol::Join(game_id)
    Host->>SB: INSERT game_players
    Host-->>P: Protocol::Ack {nation}
    Host--)Others: broadcast GameEvent::PlayerJoined
    Others->>Others: AppEvent updates Lobby feed
```

### 11c. Submitting an order — host online vs offline

```mermaid
sequenceDiagram
    participant P as Player (TUI)
    participant H as Host
    participant SB as Supabase (order_queue)

    P->>P: OrderEntryScreen FSM completes -> orders built
    alt Host reachable
        P->>H: Protocol::SubmitOrders(orders)
        H->>H: OrderCollector.collect()
        H-->>P: Protocol::Ack
    else Host unreachable
        P->>SB: POST /order_queue {status: pending}
        SB-->>P: 201 Created
        Note over P: queued, resolved next time host is up
    end
```

### 11d. Host comes back online — catch-up

```mermaid
sequenceDiagram
    participant H as Host (startup)
    participant SB as Supabase

    H->>SB: GET latest snapshot for game_id
    SB-->>H: Snapshot (GameContext JSON)
    H->>SB: GET order_queue?status=eq.pending
    SB-->>H: [PendingOrder...]
    H->>H: hydrate GameContext, feed orders into collectors
    H->>H: PhaseHandler.resolve()
    H->>SB: PATCH order_queue set status=resolved
    H->>SB: INSERT snapshots (new GameContext)
    H--)P: broadcast GameEvent::PhaseChanged
```

### 11e. Messaging — never touches the host

```mermaid
sequenceDiagram
    participant P as Player
    participant SB as Supabase (messages + Realtime)
    participant Others as Other players

    P->>SB: INSERT messages {game_id, user_id, body}
    SB--)Others: Realtime Broadcast (or short poll fallback)
    Others->>Others: AppEvent::Network(GameEvent::NewMessage)
```

---

## 12. Supabase schema (simplified)

Keep every table's shape flat and JSON-serializable — it should mirror your Rust structs 1:1 so `serde` round-trips without a translation layer.

| Table | Key columns | Notes |
|---|---|---|
| `users` | `user_id` (uuid, PK, = `auth.users.id`) · `username` · `created_at` | Password hashing is handled by Supabase Auth itself — don't roll your own. |
| `games` | `game_id` PK · `name` · `host_user_id` FK · `host_ip` · `host_port` · `status` (open/active/paused/finished) · `discoverable` bool | `discoverable=false` gives you private/invite-only games via `game_id` sharing without extra logic. |
| `game_players` | `id` PK · `game_id` FK · `user_id` FK · `nation` · `joined_at` | |
| `snapshots` | `id` PK · `game_id` FK · `turn_number` · `phase` · `game_state` jsonb · `created_at` | `game_state` is your `Board` + `GamePhase`, serde-serialized whole. |
| `order_queue` | `id` PK · `game_id` FK · `user_id` FK · `turn_number` · `orders` jsonb (`Vec<Order>`) · `status` (pending/resolved) · `created_at` | |
| `messages` | `id` PK · `game_id` FK · `user_id` FK · `body` text · `created_at` | Add this table to the `supabase_realtime` publication so Broadcast can serve it. |

**One thing that changed on Supabase's side recently and matters here:** as of May 30, 2026, new projects default to *not* auto-exposing new tables through the Data API — you now need an explicit RLS policy/grant per table before PostgREST or Realtime can see it. <cite index="1-1">Grants control whether a role can access a table at all, while RLS controls which rows that role can see</cite>. Practically: after creating each table above, add a policy (e.g. "authenticated users can select/insert their own rows") or it'll 404 through the REST API even though it exists in Postgres.

For `messages`, prefer **Broadcast** over raw Postgres Changes if you expect more than a handful of concurrent players per game — <cite index="8-1">Postgres Changes filters and re-checks RLS per connected client, which is a known scaling cliff at higher subscriber counts, whereas Broadcast sends ephemeral messages client-to-client with lower overhead</cite>. For a Diplomacy game (max 7 players/game) this genuinely won't matter, but it costs nothing to default to Broadcast now.

---

## 13. Rust features this design actually leans on

1. **Sum types for state** — `GamePhase`, `Order`, `Screen`-adjacent `Transition` are enums with data. A `SpringMovement` phase literally cannot carry retreat data; the compiler rejects invalid states that a GoF class hierarchy in Java/C++ would only catch at runtime.
2. **Exhaustive `match`** — add a new `GamePhase` variant and every `match` on it fails to compile until you handle it. This is your regression-proofing for "adjustment phase logic" style bugs.
3. **Ownership instead of locks, where possible** — `App` on the client is the *only* thing that mutates TUI state, because the network thread only ever pushes into a channel. No `Arc<Mutex<AppState>>` needed client-side. The host *does* need `Arc<RwLock<...>>` for `GameRegistry` because it's genuinely shared across concurrent connections — use the lock only where the sharing is real.
4. **`Send + Sync` as a compiler check** — anything you put in `GameHandle` behind the `Arc<RwLock<_>>` must be `Send + Sync`; the compiler enforces this before you ever ship a data race, rather than you discovering it in production.
5. **`serde` derive** — `#[derive(Serialize, Deserialize)]` on `Protocol`, `Order`, `GameContext` gets you the wire format, the Supabase JSON columns, and the snapshot format from one annotation, instead of three hand-written parsers.
6. **`thiserror` + `?`** — typed errors per crate (`OrderError`, `AuthError`) composed with `?`, collapsed to `anyhow::Error` only at the two binary entry points.
7. **Trait objects at boundaries, generics inside** — `Command<C: Client, S: SessionKeeper>` stays generic (zero-cost, monomorphized) since it's internal; `Box<dyn Screen>` and `Box<dyn PhaseHandler>` use dynamic dispatch at the points where you need a heterogeneous collection (the screen stack, the phase-to-handler lookup) — that's the correct GoF-to-Rust translation, not "everything is `dyn`."

---

## 14. Code sketches

**Screen trait + back-stack (§7):**
```rust
pub trait Screen {
    fn render(&self, app: &App, frame: &mut Frame);
    fn handle_input(&mut self, app: &mut App, key: KeyEvent) -> Transition;
}

pub enum Transition {
    Stay,
    Push(Box<dyn Screen>),
    Pop,
    Replace(Box<dyn Screen>),
}

impl App {
    pub fn apply(&mut self, t: Transition) {
        match t {
            Transition::Stay => {}
            Transition::Push(s) => self.screen_stack.push(s),
            Transition::Pop => { self.screen_stack.pop(); }
            Transition::Replace(s) => { self.screen_stack.pop(); self.screen_stack.push(s); }
        }
    }
}
```

**GameRegistry — Registry, not Singleton (§2, §9):**
```rust
#[derive(Clone)]
pub struct GameRegistry {
    games: Arc<RwLock<HashMap<GameId, GameHandle>>>,
}

impl GameRegistry {
    pub fn new() -> Self {
        Self { games: Arc::new(RwLock::new(HashMap::new())) }
    }
    // Cloned (cheap, just bumps the Arc refcount) into every tokio::spawn'd
    // connection task in main.rs — no global static, no `unsafe`.
}
```

**PhaseHandler dispatch (State + Factory Method):**
```rust
pub trait PhaseHandler: Send + Sync {
    fn validate(&self, order: &Order, board: &Board) -> Result<(), OrderError>;
    fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution;
    fn next_phase(&self, had_dislodgements: bool) -> GamePhase;
}

pub fn handler_for(phase: GamePhase) -> Box<dyn PhaseHandler> {
    match phase {
        GamePhase::SpringMovement | GamePhase::FallMovement => Box::new(MovementPhase),
        GamePhase::SpringRetreat | GamePhase::FallRetreat => Box::new(RetreatPhase),
        GamePhase::WinterBuild => Box::new(BuildPhase),
    }
}
```

**Observer via channel, not trait objects:**
```rust
// Host: GameHandle holds a broadcast::Sender<GameEvent>, cloned per subscriber.
let (tx, _) = tokio::sync::broadcast::channel::<GameEvent>(64);
tx.send(GameEvent::PlayerJoined(username))?;

// Client: network thread owns the receiving half, forwards into the TUI's mpsc channel.
while let Ok(event) = broadcast_rx.recv().await {
    app_tx.send(AppEvent::Network(event))?;
}
```

---

## 15. Suggested build order

Build bottom-up through the crates so each milestone compiles and is testable in isolation before it touches the network:

1. **`core`** — `Board`, `Order`, `GamePhase`, `PhaseHandler` + unit tests for adjudication logic. No I/O, no async.
2. **`client` skeleton** — `Screen` trait, `App`, all six screens rendering against a hardcoded fake `GameContext` (this replaces `fake_context.rs`). No network yet — validates the FSM and back-stack feel right.
3. **`host` skeleton** — `GameRegistry`, `GameHandle`, in-memory only (no Supabase). Validates order collection + phase resolution end-to-end over TCP with the real client.
4. **Wire protocol** — swap the semicolon format for `Protocol` + `serde_json`, confirm host/client still talk.
5. **Supabase: auth + discoverable games** — registration, login, game browser filtering on `discoverable=true`.
6. **Supabase: snapshots + offline queue** — `SnapshotService`, `QueuePoller`, the online/offline branch in order submission.
7. **Messaging** — `messages` table + Realtime subscription, fully decoupled from host uptime.
8. **(Optional, later)** — reintroduce the Node.js map renderer as an alternate `Screen` implementation, once everything else is stable.

Each step is independently demoable, which is the main defense against the project ballooning again.