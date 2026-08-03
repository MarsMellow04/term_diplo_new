// use std::str::FromStr;

// use diplomacy::{Nation, Phase::{self}, UnitPositions, UnitType as LibUnitType, geo::RegionKey, judge::{OrderState, Outcome, Rulebook, Submission}, order::{MainCommand, MoveCommand}};
// use diplomacy::order::{Order as LibOrder};
// // use crate::board::Board;
// use thiserror::Error;
// use crate::{board::Board, order::Order};

// // Error Classes

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

// // Enums

// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum Season {
//     Spring, 
//     Autumn,
//     Winter
// }

// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum GamePhase {
//     SpringMain,
//     SpringRetreat,
//     AutumnMain,
//     AutumnRetreat,
//     WinterBuild
// }


// impl GamePhase {
//     // Small helper functions, done this way to not allow inaccurate game phases
//     pub fn phase_kind(&self) -> Phase {
//         match self {
//             GamePhase::SpringMain | GamePhase::AutumnMain => Phase::Main,
//             GamePhase::SpringRetreat | GamePhase::AutumnRetreat => Phase::Retreat,
//             GamePhase::WinterBuild => Phase::Build
//         }
//     }

//     pub fn season(&self) -> Season {
//         match self {
//             GamePhase::SpringMain | GamePhase::SpringRetreat => Season::Spring,
//             GamePhase::AutumnMain | GamePhase::AutumnRetreat => Season::Autumn,
//             GamePhase::WinterBuild => Season::Winter
//         }
//     }

//     pub fn next(self, has_dislodgements: bool) -> Self {
//         match self {
//             GamePhase::SpringMain => {
//                 if has_dislodgements { GamePhase::SpringRetreat } else { GamePhase::AutumnMain }
//             }
//             GamePhase::SpringRetreat  => GamePhase::AutumnMain,
//             GamePhase::AutumnMain   => {
//                 if has_dislodgements { GamePhase::AutumnRetreat } else { GamePhase::WinterBuild }
//             }
//             GamePhase::AutumnRetreat    => GamePhase::WinterBuild,
//             GamePhase::WinterBuild    => GamePhase::SpringMain,
//         }
//     }
// }

// #[derive(Debug, Clone)]
// pub struct Resolution {
//     pub has_dislodgements: bool,
//     pub results: Vec<OrderResult>,
// }

// #[derive(Debug, Clone)]
// pub struct OrderResult {
//     pub nation: Nation,
//     pub unit_type: LibUnitType,
//     pub region: String,
//     pub command: String,
//     pub succeeded: bool,
// }
// use std::str::FromStr;

// use diplomacy::{Nation, Phase::{self}, UnitPositions, UnitType as LibUnitType, geo::RegionKey, judge::{OrderState, Outcome, Rulebook, Submission}, order::{MainCommand, MoveCommand}};
// use diplomacy::order::{Order as LibOrder};
// // use crate::board::Board;
// use thiserror::Error;
// use crate::{board::Board, order::Order};

// // Error Classes

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

// // Enums

// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum Season {
//     Spring, 
//     Autumn,
//     Winter
// }

// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum GamePhase {
//     SpringMain,
//     SpringRetreat,
//     AutumnMain,
//     AutumnRetreat,
//     WinterBuild
// }


// impl GamePhase {
//     // Small helper functions, done this way to not allow inaccurate game phases
//     pub fn phase_kind(&self) -> Phase {
//         match self {
//             GamePhase::SpringMain | GamePhase::AutumnMain => Phase::Main,
//             GamePhase::SpringRetreat | GamePhase::AutumnRetreat => Phase::Retreat,
//             GamePhase::WinterBuild => Phase::Build
//         }
//     }

//     pub fn season(&self) -> Season {
//         match self {
//             GamePhase::SpringMain | GamePhase::SpringRetreat => Season::Spring,
//             GamePhase::AutumnMain | GamePhase::AutumnRetreat => Season::Autumn,
//             GamePhase::WinterBuild => Season::Winter
//         }
//     }

//     pub fn next(self, has_dislodgements: bool) -> Self {
//         match self {
//             GamePhase::SpringMain => {
//                 if has_dislodgements { GamePhase::SpringRetreat } else { GamePhase::AutumnMain }
//             }
//             GamePhase::SpringRetreat  => GamePhase::AutumnMain,
//             GamePhase::AutumnMain   => {
//                 if has_dislodgements { GamePhase::AutumnRetreat } else { GamePhase::WinterBuild }
//             }
//             GamePhase::AutumnRetreat    => GamePhase::WinterBuild,
//             GamePhase::WinterBuild    => GamePhase::SpringMain,
//         }
//     }
// }

// pub struct Resolution {
//     pub has_dislodgements: bool,
//     pub results: Vec<OrderResult>,
// }

// pub trait PhaseHandler: Send + Sync {
//     /// Optional: early UI validation. Can always return Ok and let Submission catch it.
//     fn validate(&self, _order: &PlayerOrder, _board: &Board) -> Result<(), OrderError> {
//         Ok(())
//     }
    
//     fn resolve(&self, board: &mut Board, orders: Vec<PlayerOrder>) -> Resolution;
//     fn next_phase(&self, resolution: &Resolution) -> GamePhase;
// }

// // Factory
// pub fn handler_for(phase: GamePhase) -> Box<dyn PhaseHandler> {
//     match phase {
//         GamePhase::SpringMain | GamePhase::AutumnMain => Box::new(MainPhase),
//         GamePhase::SpringRetreat | GamePhase::AutumnRetreat => Box::new(RetreatPhase),
//         GamePhase::WinterBuild => Box::new(BuildPhase),
//     }
// }

// pub struct MainPhase;
// impl PhaseHandler for MainPhase {
//     fn resolve(&self, board: &mut Board, orders: Vec<PlayerOrder>) -> Resolution {
//         let lib_orders: Vec<_> = orders.into_iter().filter_map(to_main_order).collect();
        
//         // Submission::new validates AND prepares adjudication context
//         let submission = Submission::new(&board.map, &board.units, lib_orders)
//             .expect("Submission filters illegal orders");

//         // Outcome borrows submission — extract everything before returning
//         let outcome = submission.adjudicate(Rulebook::default());
        
//         let has_dislodgements = !outcome.retreat_phase_data().is_empty();
        
//         let results = outcome
//             .orders()
//             .map(|o| OrderResult {
//                 nation: o.nation().clone(),
//                 unit_type: o.unit_type(),
//                 region: o.region().to_string(),
//                 command: format!("{}", o), // or however you want to render it
//                 succeeded: OrderState::from(o.outcome()) == OrderState::Succeeds,
//             })
//             .collect();

//         // TODO: update board.units from outcome (you'll need to compute new positions)
        
//         Resolution { has_dislodgements, results }
//     }

//     fn next_phase(&self, res: &Resolution) -> GamePhase {
//         match self {
//             // You need to track current phase in the handler or pass it in
//             GamePhase::SpringMain => {
//                 if res.has_dislodgements { GamePhase::SpringRetreat } else { GamePhase::AutumnMain }
//             }
//             _ => panic!("MainPhase called with wrong phase"),
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
// use super::*;

//     #[test]
//     fn correct_phases_for_game_phase() {
//         let test_cases = [
//             (GamePhase::SpringMain, Phase::Main),
//             (GamePhase::SpringRetreat, Phase::Retreat),
//             (GamePhase::AutumnMain, Phase::Main),
//             (GamePhase::AutumnRetreat, Phase::Retreat),
//             (GamePhase::WinterBuild, Phase::Build),
//         ];

//         for (input, expected) in test_cases.iter() {
//             assert_eq!(input.phase_kind(), *expected);
//         }
//     }

//     #[test]
//     fn correct_season_for_game_phase() {
//         let test_cases = [
//             (GamePhase::SpringMain, Season::Spring),
//             (GamePhase::SpringRetreat, Season::Spring),
//             (GamePhase::AutumnMain, Season::Autumn),
//             (GamePhase::AutumnRetreat, Season::Autumn),
//             (GamePhase::WinterBuild, Season::Winter),
//         ];

//         for (input, expected) in test_cases.iter() {
//             assert_eq!(input.season(), *expected);
//         }
//     }

//     #[test]
//     fn next_game_phase_has_dislodge() {
//         let test_cases = [
//             (GamePhase::SpringMain, GamePhase::SpringRetreat),
//             (GamePhase::SpringRetreat, GamePhase::AutumnMain),
//             (GamePhase::AutumnMain, GamePhase::AutumnRetreat),
//             (GamePhase::AutumnRetreat, GamePhase::WinterBuild),
//             (GamePhase::WinterBuild, GamePhase::SpringMain),
//         ];

//         for (input, expected) in test_cases.iter() {
//             assert_eq!(input.next(true), *expected);
//         }
//     }

//     #[test]
//     fn next_game_phase_no_dislodge() {
//         let test_cases = [
//             (GamePhase::SpringMain, GamePhase::AutumnMain),
//             (GamePhase::AutumnMain, GamePhase::WinterBuild),
//             (GamePhase::WinterBuild, GamePhase::SpringMain),
//         ];

//         for (input, expected) in test_cases.iter() {
//             assert_eq!(input.next(false), *expected);
//         }
//     }
// }

use std::str::FromStr;

use diplomacy::{
    Nation, UnitType as LibUnitType, geo::RegionKey, judge::{OrderState, Rulebook, Submission}, order::{MainCommand, MoveCommand, Order as LibOrder},
};
use thiserror::Error;

use crate::{board::Board, order::{Order, UnitType}};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum PhaseError {
    #[error("invalid phase transition")]
    InvalidTransition,
}

#[derive(Error, Debug)]
pub enum OrderError {
    #[error("order is illegal in the current phase")]
    Illegal,
    #[error("order validation failed: {0}")]
    ValidationFailed(String),
}

// ---------------------------------------------------------------------------
// Game Phase
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Spring,
    Autumn,
    Winter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    SpringMain,
    SpringRetreat,
    AutumnMain,
    AutumnRetreat,
    WinterBuild,
}

impl GamePhase {
    pub fn phase_kind(&self) -> diplomacy::Phase {
        match self {
            GamePhase::SpringMain | GamePhase::AutumnMain => diplomacy::Phase::Main,
            GamePhase::SpringRetreat | GamePhase::AutumnRetreat => diplomacy::Phase::Retreat,
            GamePhase::WinterBuild => diplomacy::Phase::Build,
        }
    }

    pub fn season(&self) -> Season {
        match self {
            GamePhase::SpringMain | GamePhase::SpringRetreat => Season::Spring,
            GamePhase::AutumnMain | GamePhase::AutumnRetreat => Season::Autumn,
            GamePhase::WinterBuild => Season::Winter,
        }
    }

    pub fn next(self, has_dislodgements: bool) -> Self {
        match self {
            GamePhase::SpringMain => {
                if has_dislodgements {
                    GamePhase::SpringRetreat
                } else {
                    GamePhase::AutumnMain
                }
            }
            GamePhase::SpringRetreat => GamePhase::AutumnMain,
            GamePhase::AutumnMain => {
                if has_dislodgements {
                    GamePhase::AutumnRetreat
                } else {
                    GamePhase::WinterBuild
                }
            }
            GamePhase::AutumnRetreat => GamePhase::WinterBuild,
            GamePhase::WinterBuild => GamePhase::SpringMain,
        }
    }
}

// ---------------------------------------------------------------------------
// Resolution — owned, serializable, no lifetimes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Resolution {
    pub has_dislodgements: bool,
    pub results: Vec<OrderResult>,
}

#[derive(Debug, Clone)]
pub struct OrderResult {
    pub nation: Nation,
    pub unit_type: LibUnitType,
    pub region: String,
    pub command: String,
    pub succeeded: bool,
}

// ---------------------------------------------------------------------------
// PhaseHandler trait
// ---------------------------------------------------------------------------

pub trait PhaseHandler: Send + Sync {
    /// Optional pre-flight check for the TUI. The real validation happens
    /// inside Submission::new, which rejects illegal orders and auto-generates holds.
    fn validate(&self, _order: &Order, _board: &Board) -> Result<(), OrderError> {
        Ok(())
    }

    fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution;

    /// Default implementation delegates to GamePhase::next.
    fn next_phase(&self, current: GamePhase, resolution: &Resolution) -> GamePhase {
        current.next(resolution.has_dislodgements)
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub fn handler_for(phase: GamePhase) -> Box<dyn PhaseHandler> {
    match phase.phase_kind() {
        diplomacy::Phase::Main => Box::new(MainPhase),
        diplomacy::Phase::Retreat => Box::new(RetreatPhase),
        diplomacy::Phase::Build => Box::new(BuildPhase),
    }
}

// ---------------------------------------------------------------------------
// Main Phase
// ---------------------------------------------------------------------------

pub struct MainPhase;

impl PhaseHandler for MainPhase {
    fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution {
        let lib_orders: Vec<LibOrder<RegionKey, MainCommand<RegionKey>>> = orders
            .iter()
            .filter_map(to_main_order)
            .collect();

        let submission = Submission::new(&*board.map, &board.units, lib_orders);

        // Outcome borrows submission — extract everything before returning
        let outcome = submission.adjudicate(Rulebook::default());

        // `needs_player_input()` reports whether a dislodged unit has *no* legal
        // retreat (forced disband), not whether any dislodgement happened at all —
        // it is vacuously false when nothing was dislodged. Dislodgement itself is
        // whether the map returned by `dislodged()` is non-empty.
        let has_dislodgements = !outcome.to_retreat_start().dislodged().is_empty();

        let results = submission
            .submitted_orders()
            .into_iter()
            .map(|o| {
                let state: OrderState = outcome.get(o).unwrap().into();
                OrderResult {
                    nation: o.nation.clone(),
                    unit_type: o.unit_type.clone(),
                    region: o.region.to_string(),
                    command: format!("{}", o),
                    succeeded: state == OrderState::Succeeds,
                }
            })
            .collect();

        // TODO: rebuild board.units from outcome after adjudication
        // board.units = extract_new_positions(&outcome);

        Resolution {
            has_dislodgements,
            results,
        }
    }
}

// ---------------------------------------------------------------------------
// Retreat Phase — stub
// ---------------------------------------------------------------------------

pub struct RetreatPhase;

impl PhaseHandler for RetreatPhase {
    fn resolve(&self, _board: &mut Board, _orders: Vec<Order>) -> Resolution {
        todo!("RetreatPhase::resolve not yet implemented")
    }
}

// ---------------------------------------------------------------------------
// Build Phase — stub
// ---------------------------------------------------------------------------

pub struct BuildPhase;

impl PhaseHandler for BuildPhase {
    fn resolve(&self, _board: &mut Board, _orders: Vec<Order>) -> Resolution {
        todo!("BuildPhase::resolve not yet implemented")
    }
}

fn map_unit_type(ut: UnitType) -> LibUnitType {
    match ut {
        UnitType::Army => LibUnitType::Army,
        UnitType::Fleet => LibUnitType::Fleet,
    }
}

/// `diplomacy::Nation` is a bare string wrapper with byte-exact `PartialEq` — it does
/// no case folding. Board units are represented with uppercase nation codes (matching
/// the crate's own DATC-style notation, e.g. `"AUS: A tri"`), so any order nation must
/// be normalized to the same case or `Submission` will treat the order as belonging to
/// a foreign/nonexistent unit and silently discard it.
fn to_nation(raw: &str) -> Nation {
    Nation::from(raw.to_uppercase().as_str())
}

fn to_main_order(order: &Order) -> Option<LibOrder<RegionKey, MainCommand<RegionKey>>> {
    match order {
        Order::Move { unit, target } => {
            let nation = to_nation(&unit.nation);
            let region = RegionKey::from_str(&unit.region).unwrap();  // ← from_name, not parse
            let target = RegionKey::from_str(target).unwrap();        // ← from_name, not parse
            let unit_type = map_unit_type(unit.unit_type);      // ← direct map, not from_str
            Some(LibOrder::new(
                nation,
                unit_type,
                region,
                MainCommand::Move(MoveCommand::new(target)),
            ))
        }
        Order::Hold { unit } => {
            let nation = to_nation(&unit.nation);
            let region = RegionKey::from_str(&unit.region).unwrap();
            let unit_type = map_unit_type(unit.unit_type);
            Some(LibOrder::new(nation, unit_type, region, MainCommand::Hold))
        }
        // Support / Convoy left for later
        _ => None,
    }
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use diplomacy::geo;
    use diplomacy::UnitPosition;

    use super::*;
    use crate::board::Board;
    use crate::order::{Order, UnitRef, UnitType};

    /// Parse a unit-position string exactly like the Ted Driggs DATC tests.
    fn unit_pos(s: &'static str) -> UnitPosition<'static, RegionKey> {
        s.parse().unwrap()
    }

    // -----------------------------------------------------------------------
    // Phase logic tests (your existing ones, kept for coverage)
    // -----------------------------------------------------------------------

    #[test]
    fn correct_phases_for_game_phase() {
        let test_cases = [
            (GamePhase::SpringMain, diplomacy::Phase::Main),
            (GamePhase::SpringRetreat, diplomacy::Phase::Retreat),
            (GamePhase::AutumnMain, diplomacy::Phase::Main),
            (GamePhase::AutumnRetreat, diplomacy::Phase::Retreat),
            (GamePhase::WinterBuild, diplomacy::Phase::Build),
        ];

        for (input, expected) in test_cases.iter() {
            assert_eq!(input.phase_kind(), *expected);
        }
    }

    #[test]
    fn correct_season_for_game_phase() {
        let test_cases = [
            (GamePhase::SpringMain, Season::Spring),
            (GamePhase::SpringRetreat, Season::Spring),
            (GamePhase::AutumnMain, Season::Autumn),
            (GamePhase::AutumnRetreat, Season::Autumn),
            (GamePhase::WinterBuild, Season::Winter),
        ];

        for (input, expected) in test_cases.iter() {
            assert_eq!(input.season(), *expected);
        }
    }

    #[test]
    fn next_game_phase_has_dislodge() {
        let test_cases = [
            (GamePhase::SpringMain, GamePhase::SpringRetreat),
            (GamePhase::SpringRetreat, GamePhase::AutumnMain),
            (GamePhase::AutumnMain, GamePhase::AutumnRetreat),
            (GamePhase::AutumnRetreat, GamePhase::WinterBuild),
            (GamePhase::WinterBuild, GamePhase::SpringMain),
        ];

        for (input, expected) in test_cases.iter() {
            assert_eq!(input.next(true), *expected);
        }
    }

    #[test]
    fn next_game_phase_no_dislodge() {
        let test_cases = [
            (GamePhase::SpringMain, GamePhase::AutumnMain),
            (GamePhase::AutumnMain, GamePhase::WinterBuild),
            (GamePhase::WinterBuild, GamePhase::SpringMain),
        ];

        for (input, expected) in test_cases.iter() {
            assert_eq!(input.next(false), *expected);
        }
    }

    // -----------------------------------------------------------------------
    // Adjudication tests (Ted Driggs style)
    // -----------------------------------------------------------------------

    #[test]
    fn main_phase_hold_succeeds() {
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("ITA: A ven")],
            ownership: HashMap::new(),
        };

        let orders = vec![Order::Hold {
            unit: UnitRef {
                nation: "ita".into(),
                unit_type: UnitType::Army,
                region: "ven".into(),
            },
        }];

        let resolution = MainPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 1, "one order should be resolved");
        assert!(resolution.results[0].succeeded, "hold should succeed");
        assert!(!resolution.has_dislodgements, "hold does not dislodge");
    }

    #[test]
    fn main_phase_successful_move() {
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("AUS: A tri")],
            ownership: HashMap::new(),
        };

        let orders = vec![Order::Move {
            unit: UnitRef {
                nation: "aus".into(),
                unit_type: UnitType::Army,
                region: "tri".into(),
            },
            target: "ven".into(),
        }];

        let resolution = MainPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 1);
        assert!(
            resolution.results[0].succeeded,
            "unopposed move into empty province should succeed"
        );
        assert!(!resolution.has_dislodgements);
    }

    #[test]
    fn main_phase_simple_bounce() {
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![
                unit_pos("AUS: A vie"),
                unit_pos("ITA: A ven"),
            ],
            ownership: HashMap::new(),
        };

        let orders = vec![
            Order::Move {
                unit: UnitRef {
                    nation: "aus".into(),
                    unit_type: UnitType::Army,
                    region: "vie".into(),
                },
                target: "tyr".into(),
            },
            Order::Move {
                unit: UnitRef {
                    nation: "ita".into(),
                    unit_type: UnitType::Army,
                    region: "ven".into(),
                },
                target: "tyr".into(),
            },
        ];

        let resolution = MainPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 2, "both orders should appear in results");
        assert!(
            resolution.results.iter().all(|r| !r.succeeded),
            "both moves should bounce and fail"
        );
        assert!(
            !resolution.has_dislodgements,
            "a bounce does not dislodge anyone"
        );
    }

    #[test]
    fn main_phase_head_to_head_equal_strength() {
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![
                unit_pos("GER: A ber"),
                unit_pos("GER: A kie"),
            ],
            ownership: HashMap::new(),
        };

        let orders = vec![
            Order::Move {
                unit: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "ber".into(),
                },
                target: "kie".into(),
            },
            Order::Move {
                unit: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "kie".into(),
                },
                target: "ber".into(),
            },
        ];

        let resolution = MainPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 2);
        assert!(
            resolution.results.iter().all(|r| !r.succeeded),
            "head-to-head with equal strength should fail for both"
        );
        assert!(!resolution.has_dislodgements);
    }
}