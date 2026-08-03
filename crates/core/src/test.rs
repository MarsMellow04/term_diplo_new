// use diplomacy::geo::{Map, RegionKey};
// use diplomacy::judge::{self, OrderState, StandardRulebook, Submission};
// use diplomacy::order::{Command, Order};
// use diplomacy::{Nation, UnitType};

// // Your own type so you can add serde, game_id, etc.
// pub struct GameContext {
//     pub map: Map,                    // from diplomacy crate
//     pub units: Vec<UnitPosition>,    // current board state
//     pub phase: GamePhase,
//     pub year: u16,
// }

// pub enum GamePhase {
//     SpringMovement,
//     SpringRetreat,
//     FallMovement,
//     FallRetreat,
//     WinterBuild,
// }

// // Resolution wraps the library's phase-specific outcomes
// pub enum Resolution {
//     Main(judge::Outcome<'static, StandardRulebook>),
//     Retreat(judge::retreat::Outcome),
//     Build(judge::build::Outcome),
// }

// pub trait PhaseHandler: Send + Sync {
//     fn validate(&self, order: &Order<Nation, RegionKey>, ctx: &GameContext) -> Result<(), OrderError>;
//     fn resolve(&self, ctx: &mut GameContext, orders: Vec<Order<Nation, RegionKey>>) -> Resolution;
//     fn next_phase(&self, resolution: &Resolution) -> GamePhase;
// }

// // --- Movement phase implementation ---
// pub struct MovementPhase;

// impl PhaseHandler for MovementPhase {
//     fn validate(&self, order: &Order<Nation, RegionKey>, _ctx: &GameContext) -> Result<(), OrderError> {
//         // The library's Submission::new does the heavy lifting; 
//         // this is for pre-flight UI validation if you want it.
//         Ok(())
//     }

//     fn resolve(&self, ctx: &mut GameContext, orders: Vec<Order<Nation, RegionKey>>) -> Resolution {
//         // 1. Build the library's submission from current board + orders
//         let submission = Submission::new(&ctx.map, ctx.units.clone(), orders)
//             .expect("Submission handles illegal orders internally");

//         // 2. Adjudicate using standard rules
//         let outcome = submission.adjudicate(StandardRulebook);

//         // 3. Apply the outcome to your GameContext units
//         //    (You'd write a helper that translates Outcome → new UnitPositions)
//         ctx.units = apply_main_outcome(&ctx.map, &outcome);

//         Resolution::Main(outcome)
//     }

//     fn next_phase(&self, resolution: &Resolution) -> GamePhase {
//         // Use the library's Outcome::retreat_phase_data() to know if retreats are needed
//         match resolution {
//             Resolution::Main(outcome) => {
//                 if outcome.retreat_phase_data().is_empty() {
//                     // No dislodgements → next movement or build
//                     GamePhase::FallMovement // or WinterBuild, depending on season
//                 } else {
//                     GamePhase::SpringRetreat // or FallRetreat
//                 }
//             }
//             _ => panic!("MovementPhase given non-main resolution"),
//         }
//     }
// }


// use thiserror::Error;

// // ---------------------------------------------------------------------------
// // Phase domain — illegal combinations are unrepresentable
// // ---------------------------------------------------------------------------

// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// pub enum GamePhase {
//     SpringMovement,
//     SpringRetreat,
//     FallMovement,
//     FallRetreat,
//     WinterBuild,
// }

// impl GamePhase {
//     /// View into the season component (for display, serialization, etc.)
//     pub fn season(&self) -> Season {
//         match self {
//             GamePhase::SpringMovement | GamePhase::SpringRetreat => Season::Spring,
//             GamePhase::FallMovement   | GamePhase::FallRetreat   => Season::Fall,
//             GamePhase::WinterBuild    => Season::Winter,
//         }
//     }

//     /// View into the phase-of-season component
//     pub fn phase_kind(&self) -> PhaseKind {
//         match self {
//             GamePhase::SpringMovement | GamePhase::FallMovement => PhaseKind::Movement,
//             GamePhase::SpringRetreat  | GamePhase::FallRetreat  => PhaseKind::Retreat,
//             GamePhase::WinterBuild    => PhaseKind::Build,
//         }
//     }

//     /// Natural progression per Diplomacy rules.
//     /// `has_dislodgements` is only meaningful for Movement phases.
//     pub fn next(self, has_dislodgements: bool) -> Self {
//         match self {
//             GamePhase::SpringMovement => {
//                 if has_dislodgements { GamePhase::SpringRetreat } else { GamePhase::FallMovement }
//             }
//             GamePhase::SpringRetreat  => GamePhase::FallMovement,
//             GamePhase::FallMovement   => {
//                 if has_dislodgements { GamePhase::FallRetreat } else { GamePhase::WinterBuild }
//             }
//             GamePhase::FallRetreat    => GamePhase::WinterBuild,
//             GamePhase::WinterBuild    => GamePhase::SpringMovement,
//         }
//     }
// }

// /// Season is a *view*, not an authority. You cannot construct a GamePhase from
// /// a Season alone — that would re-introduce the validation problem.
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum Season {
//     Spring,
//     Fall,
//     Winter,
// }

// /// Same here: a read-only label for UI / protocol, not a constructor input.
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum PhaseKind {
//     Movement,
//     Retreat,
//     Build,
// }

// // ---------------------------------------------------------------------------
// // Errors
// // ---------------------------------------------------------------------------

// #[derive(Error, Debug)]
// pub enum PhaseError {
//     #[error("invalid phase transition")]
//     InvalidTransition,
// }

// #[derive(Error, Debug)]
// pub enum OrderError {
//     #[error("order is illegal in the current phase")]
//     Illegal,
//     #[error("order validation failed: {0}")]
//     ValidationFailed(String),
// }

// // ---------------------------------------------------------------------------
// // PhaseHandler trait — the State pattern from your architecture doc
// // ---------------------------------------------------------------------------

// pub trait PhaseHandler: Send + Sync {
//     /// Pre-flight check (e.g. "armies can't move to sea"). The Ted Driggs
//     /// library's Submission::new does the heavy lifting; this is for early
//     /// UI feedback before the player hits Submit.
//     fn validate(&self, order: &Order, board: &Board) -> Result<(), OrderError>;

//     /// Run adjudication and return the new board state + resolution details.
//     fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution;

//     /// Which phase follows this one? Delegates to GamePhase::next.
//     fn next_phase(&self, resolution: &Resolution) -> GamePhase;
// }

// // Concrete implementations are selected by handler_for(phase) — Factory Method
// pub fn handler_for(phase: GamePhase) -> Box<dyn PhaseHandler> {
//     match phase {
//         GamePhase::SpringMovement | GamePhase::FallMovement => Box::new(MovementPhase),
//         GamePhase::SpringRetreat  | GamePhase::FallRetreat  => Box::new(RetreatPhase),
//         GamePhase::WinterBuild    => Box::new(BuildPhase),
//     }
// }