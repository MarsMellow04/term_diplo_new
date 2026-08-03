use std::{borrow::Cow, todo};
use std::collections::HashMap;
use std::str::FromStr;

use diplomacy::{
    Nation, Unit, UnitPosition, UnitPositions, UnitType as LibUnitType, 
    geo::RegionKey, 
    judge::{OrderState, Rulebook, Submission, retreat}, 
    judge::build::Submission as BldSubmission,
    order::{BuildCommand, ConvoyedMove, MainCommand, MoveCommand, Order as LibOrder, RetreatCommand, SupportedOrder},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{board::Board, order::{Order, UnitType}};

#[derive(Error, Debug)]
pub enum OrderError {
    #[error("order is illegal in the current phase")]
    Illegal,
    #[error("order validation failed: {0}")]
    ValidationFailed(String),
}

// Game Phase

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Spring,
    Autumn,
    Winter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize,)]
pub enum GamePhase {
/// Library Enum for the different rounds of a diplomacy game. 
/// Can find the phase, or season to use with the polymorphic PhaseHandler.
/// Only use next() to iterate to the next stage.
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

// Resolution — owned, serializable, no lifetimes

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
/// Resolution has been made to stop the stupid Lifetime issues.
/// The command should be able to immediatley parse the Order Result into a LibOrder when needed,
/// TODO!: Move this order reuslt over and a way to convert them.
    pub has_dislodgements: bool,
    pub results: Vec<OrderResult>,
    /// Owned snapshot of dislodgement data from a main-phase adjudication
    pub retreat_snapshot: Option<RetreatSnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetreatSnapshot {
    dislodged: Vec<(LibOrder<RegionKey, MainCommand<RegionKey>>, LibOrder<RegionKey, MainCommand<RegionKey>>)>,
    retreat_destinations: Vec<(UnitPosition<'static, RegionKey>, Vec<(RegionKey, retreat::DestStatus)>)>,
    unit_positions: Vec<UnitPosition<'static, RegionKey>>,
}

pub (crate) fn owned_unit_position(pos: &UnitPosition<'_, &RegionKey>) -> UnitPosition<'static, RegionKey> {
    UnitPosition::new(
        Unit::new(Cow::Owned(pos.nation().clone()), pos.unit.unit_type()),
        pos.region.clone(),
    )
}

impl RetreatSnapshot {
    fn from_start(start: &retreat::Start<'_>) -> Self {
        let dislodged = start
            .dislodged()
            .iter()
            .map(|(&dislodged_order, &dislodger)| (dislodged_order.clone(), dislodger.clone()))
            .collect();

        let retreat_destinations = start
            .retreat_destinations()
            .iter()
            .map(|(pos, dests)| {
                let owned_dests = dests
                    .clone()
                    .into_iter()
                    .map(|(region, status)| (region.clone(), status))
                    .collect();
                (owned_unit_position(pos), owned_dests)
            })
            .collect();

        let unit_positions = start.unit_positions().iter().map(owned_unit_position).collect();

        Self {
            dislodged,
            retreat_destinations,
            unit_positions,
        }
    }

    pub fn dislodged_count(&self) -> usize {
        self.dislodged.len()
    }

    fn to_start(&self) -> retreat::Start<'_> {
        let dislodged: HashMap<_, _> = self
            .dislodged
            .iter()
            .map(|(dislodged_order, dislodger)| (dislodged_order, dislodger))
            .collect();

        let retreat_destinations: HashMap<_, _> = self
            .retreat_destinations
            .iter()
            .map(|(pos, dests)| {
                let key = UnitPosition::new(pos.unit.clone(), &pos.region);
                let dests: Vec<(&RegionKey, retreat::DestStatus)> =
                    dests.iter().map(|(region, status)| (region, *status)).collect();
                (key, dests)
            })
            .collect();

        let unit_positions: Vec<_> = self
            .unit_positions
            .iter()
            .map(|pos| UnitPosition::new(pos.unit.clone(), &pos.region))
            .collect();

        unsafe { retreat::Start::from_raw_parts(dislodged, retreat_destinations, unit_positions) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResult {
    pub nation: Nation,
    pub unit_type: LibUnitType,
    pub region: String,
    pub command: String,
    pub succeeded: bool,
}

// PhaseHandler trait
pub trait PhaseHandler: Send + Sync {
    /// Send and sync used so it can be shared between threads.
    fn validate(&self, _order: &Order, _board: &Board) -> Result<(), OrderError> {
        Ok(())
    }

    fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution;

    /// Default implementation delegates to GamePhase::next.
    fn next_phase(&self, current: GamePhase, resolution: &Resolution) -> GamePhase {
        current.next(resolution.has_dislodgements)
    }
}

// Factory
pub fn handler_for(phase: GamePhase) -> Box<dyn PhaseHandler> {
    match phase.phase_kind() {
        diplomacy::Phase::Main => Box::new(MainPhase),
        diplomacy::Phase::Retreat => Box::new(RetreatPhase),
        diplomacy::Phase::Build => Box::new(BuildPhase),
    }
}

// Main Phase
pub struct MainPhase;

impl PhaseHandler for MainPhase {
    fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution {
        let lib_orders: Vec<LibOrder<RegionKey, MainCommand<RegionKey>>> = orders
            .iter()
            .filter_map(to_main_order)
            .collect();

        let submission = Submission::new(&board.map, &board.units, lib_orders);

        // Outcome borrows submission — extract everything before returning
        let outcome = submission.adjudicate(Rulebook::default());

        let start = outcome.to_retreat_start();
        let snapshot = RetreatSnapshot::from_start(&start);
        let has_dislodgements = snapshot.dislodged_count() > 0;

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

        let new_positions: Vec<UnitPosition<'static, RegionKey>> = start
            .unit_positions()
            .iter()
            .map(owned_unit_position)
            .collect();

        // Give snapshot to the board, should be used by the board in the retreat phase next.
        board.pending_retreat = Some(snapshot.clone());
        let _ = board.update_new_positions(new_positions);

        Resolution {
            has_dislodgements,
            results,
            retreat_snapshot: Some(snapshot),
        }
    }
}

// Retreat Phase

pub struct RetreatPhase;

impl PhaseHandler for RetreatPhase {
    fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution {
        let lib_orders: Vec<LibOrder<RegionKey, RetreatCommand<RegionKey>>> = orders
            .iter()
            .filter_map(to_retreat_order)
            .collect();

        // Rehydrate the snapshot to use during the retreat phase, 
        let snapshot = board
            .pending_retreat
            .as_ref()
            .expect("RetreatPhase::resolve called without a snapshot from a preceding MainPhase::resolve");

        let start = snapshot.to_start();
        let context = retreat::Context::new(&start, lib_orders);
        let outcome = context.resolve();

        let results = outcome
            .order_outcomes()
            .map(|(o, out)| {
                let state: OrderState = out.into();
                OrderResult {
                    nation: o.nation.clone(),
                    unit_type: o.unit_type.clone(),
                    region: o.region.to_string(),
                    command: format!("{}", o),
                    succeeded: state == OrderState::Succeeds,
                }
            })
            .collect();

        let new_positions: Vec<UnitPosition<'static, RegionKey>> = outcome
            .unit_positions()
            .iter()
            .map(owned_unit_position)
            .collect();

        // Retreats have been completed
        board.pending_retreat = None;
        let _ = board.update_new_positions(new_positions);

        Resolution {
            has_dislodgements: false,
            results,
            retreat_snapshot: None,
        }
    }
}

// Build Phase

pub struct BuildPhase;

impl PhaseHandler for BuildPhase {
    fn resolve(&self, board: &mut Board, orders: Vec<Order>) -> Resolution {
        let lib_orders: Vec<LibOrder<RegionKey, BuildCommand>> = orders
            .iter()
            .filter_map(to_build_order)
            .collect();
        let most_recent_history_stack = board.board_history.pop().expect("Should");
        let submission  = BldSubmission::new(&board.map, &most_recent_history_stack, board, lib_orders);
        let outcome = submission.adjudicate(Rulebook::default());

        let results = outcome.order_outcomes()
            .into_iter()
            .map(|(ord, _out)| {
                let state: OrderState = outcome.get(ord).unwrap().into();
                OrderResult {
                    nation: ord.nation.clone(),
                    unit_type: ord.unit_type.clone(),
                    region: ord.region.to_string(),
                    command: format!("{}", ord),
                    succeeded: state == OrderState::Succeeds,
                }
            })
            .collect();

        let new_positions: Vec<UnitPosition<'static, RegionKey>> =
            outcome.to_final_unit_positions().collect();

        let _ = board.update_new_positions(new_positions);

        Resolution {
            has_dislodgements: false,
            results,
            retreat_snapshot: None,
        }
    }
}

fn map_unit_type(ut: UnitType) -> LibUnitType {
    match ut {
        UnitType::Army => LibUnitType::Army,
        UnitType::Fleet => LibUnitType::Fleet,
    }
}
// Wrapper function for string slice to Nations
fn to_nation(raw: &str) -> Nation {
    Nation::from(raw.to_uppercase().as_str())
}

fn to_main_order(order: &Order) -> Option<LibOrder<RegionKey, MainCommand<RegionKey>>> {
    match order {
        Order::Move { unit, target } => {
            let nation = to_nation(&unit.nation);
            let region = RegionKey::from_str(&unit.region).unwrap();
            let target = RegionKey::from_str(target).unwrap();
            let unit_type = map_unit_type(unit.unit_type);
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
        
        Order::Support { unit, supported, target } => {
            let nation = to_nation(&unit.nation);
            let region = RegionKey::from_str(&unit.region).unwrap();
            let unit_type = map_unit_type(unit.unit_type);
            let target = RegionKey::from_str(target).unwrap();
            let supported_origin = RegionKey::from_str(&supported.region).unwrap();
            let supported_unit_type = map_unit_type(supported.unit_type);

            let support_order_type = if supported_origin == target {
                // If the supported region is the same as the target than it is a support hold
                MainCommand::Support(SupportedOrder::Hold(supported_unit_type, target))
            } else {
                MainCommand::Support(SupportedOrder::Move(supported_unit_type, supported_origin, target))
            };

            Some(LibOrder::new(nation, unit_type, region, support_order_type))
        }

        Order::Convoy { unit, army, target } => {
            let nation = to_nation(&unit.nation);
            let region = RegionKey::from_str(&unit.region).unwrap();
            let unit_type = map_unit_type(unit.unit_type); 
            let army_location = RegionKey::from_str(&army.region).unwrap();
            let target = RegionKey::from_str(target).unwrap();

            Some(LibOrder::new(nation, unit_type, region, MainCommand::Convoy(ConvoyedMove::new(army_location, target))))
        }
        _ => None,
    }
}

fn to_retreat_order(order: &Order) -> Option<LibOrder<RegionKey, RetreatCommand<RegionKey>>> {
    match order {
        Order::Retreat { unit, target } => {
                let nation = to_nation(&unit.nation);
                let region = RegionKey::from_str(&unit.region).unwrap();
                let unit_type = map_unit_type(unit.unit_type); 
                let target = RegionKey::from_str(&target).unwrap();

                // TODO! There is also a retreat hold command I have no idea when that is used.
                Some(LibOrder::new(nation, unit_type, region, RetreatCommand::Move(target)))
            }
        _ => None
    }
}

fn to_build_order(order: &Order) -> Option<LibOrder<RegionKey, BuildCommand>> {
    match order {
        Order::Build { nation, territory, unit_type } => {
            let nation = to_nation(nation);
            let territory = RegionKey::from_str(territory).unwrap();
            let unit_type = map_unit_type(*unit_type); 

            Some(LibOrder::new(nation, unit_type, territory, BuildCommand::Build))
        }
        Order::Disband { unit } => {
            let nation = to_nation(&unit.nation);
            let unit_type = map_unit_type(unit.unit_type);
            let region = RegionKey::from_str(&unit.region).unwrap();
            Some(LibOrder::new(nation, unit_type, region, BuildCommand::Disband))
        }

        _ => None
    }
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use std::assert_ne;
use std::collections::HashMap;
    use std::sync::Arc;

    use diplomacy::geo;
    use diplomacy::judge::build;
    use diplomacy::UnitPosition;

    use super::*;
    use crate::board::Board;
    use crate::order::{Order, UnitRef, UnitType};

    /// Parse a unit-position string exactly like the Ted Driggs DATC tests.
    fn unit_pos(s: &'static str) -> UnitPosition<'static, RegionKey> {
        s.parse().unwrap()
    }

    // Phase logic tests

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

    #[test]
    fn to_main_order_maps_all_order_kinds() {
        let test_cases = vec![
            (
                "move",
                Order::Move {
                    unit: UnitRef {
                        nation: "aus".into(),
                        unit_type: UnitType::Army,
                        region: "tri".into(),
                    },
                    target: "ven".into(),
                },
                Some(LibOrder::new(
                    Nation::from("AUS"),
                    LibUnitType::Army,
                    RegionKey::from_str("tri").unwrap(),
                    MainCommand::Move(MoveCommand::new(RegionKey::from_str("ven").unwrap())),
                )),
            ),
            (
                "hold",
                Order::Hold {
                    unit: UnitRef {
                        nation: "ita".into(),
                        unit_type: UnitType::Fleet,
                        region: "nap".into(),
                    },
                },
                Some(LibOrder::new(
                    Nation::from("ITA"),
                    LibUnitType::Fleet,
                    RegionKey::from_str("nap").unwrap(),
                    MainCommand::Hold,
                )),
            ),
            (
                // Supporter (army) and supported unit (fleet) are deliberately different
                // types: SupportedOrder must carry the *supported* unit's type, not the
                // supporter's, or the crate won't match this support to the real order.
                "support hold",
                Order::Support {
                    unit: UnitRef {
                        nation: "ger".into(),
                        unit_type: UnitType::Army,
                        region: "ber".into(),
                    },
                    supported: UnitRef {
                        nation: "ger".into(),
                        unit_type: UnitType::Fleet,
                        region: "kie".into(),
                    },
                    target: "kie".into(),
                },
                Some(LibOrder::new(
                    Nation::from("GER"),
                    LibUnitType::Army,
                    RegionKey::from_str("ber").unwrap(),
                    MainCommand::Support(SupportedOrder::Hold(
                        LibUnitType::Fleet,
                        RegionKey::from_str("kie").unwrap(),
                    )),
                )),
            ),
            (
                "support move",
                Order::Support {
                    unit: UnitRef {
                        nation: "ger".into(),
                        unit_type: UnitType::Fleet,
                        region: "kie".into(),
                    },
                    supported: UnitRef {
                        nation: "ger".into(),
                        unit_type: UnitType::Army,
                        region: "ber".into(),
                    },
                    target: "pru".into(),
                },
                Some(LibOrder::new(
                    Nation::from("GER"),
                    LibUnitType::Fleet,
                    RegionKey::from_str("kie").unwrap(),
                    MainCommand::Support(SupportedOrder::Move(
                        LibUnitType::Army,
                        RegionKey::from_str("ber").unwrap(),
                        RegionKey::from_str("pru").unwrap(),
                    )),
                )),
            ),
            (
                "convoy",
                Order::Convoy {
                    unit: UnitRef {
                        nation: "eng".into(),
                        unit_type: UnitType::Fleet,
                        region: "nth".into(),
                    },
                    army: UnitRef {
                        nation: "eng".into(),
                        unit_type: UnitType::Army,
                        region: "lon".into(),
                    },
                    target: "hol".into(),
                },
                Some(LibOrder::new(
                    Nation::from("ENG"),
                    LibUnitType::Fleet,
                    RegionKey::from_str("nth").unwrap(),
                    MainCommand::Convoy(ConvoyedMove::new(
                        RegionKey::from_str("lon").unwrap(),
                        RegionKey::from_str("hol").unwrap(),
                    )),
                )),
            ),
        ];

        for (label, input, expected) in test_cases {
            assert_eq!(to_main_order(&input), expected, "mismatch for {label} order");
        }
    }

    #[test]
    fn main_phase_handler_generic_validate() {
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("ITA: A ven")],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![]
        };

        let orders = vec![Order::Hold {
            unit: UnitRef {
                nation: "ita".into(),
                unit_type: UnitType::Army,
                region: "ven".into(),
            },
        }];

        let handler = handler_for(GamePhase::AutumnMain);
        let is_valid = handler.validate(&orders[0], &board);

        assert!(is_valid.is_ok(), "hold should succeed");

        let resolution = handler.resolve(&mut board, orders);
        assert_eq!(resolution.results.len(), 1, "one order should be resolved");

        let next_phase = handler.next_phase(GamePhase::AutumnMain, &resolution);
        assert_eq!(next_phase, GamePhase::WinterBuild, "should move onto next phase");
    }

    #[test]
    fn retreat_phase_handler_generic_validate() {
        // Chains handler_for(SpringMain) -> handler_for(SpringRetreat) -> handler_for(AutumnMain)
        // to check the factory dispatches the right concrete handler at each step of a
        // dislodgement/retreat pipeline, not just in isolation.
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![
                unit_pos("GER: A ber"),
                unit_pos("GER: A mun"),
                unit_pos("FRA: A kie"),
            ],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![],
        };

        let main_orders = vec![
            Order::Move {
                unit: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "ber".into(),
                },
                target: "kie".into(),
            },
            Order::Support {
                unit: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "mun".into(),
                },
                supported: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "ber".into(),
                },
                target: "kie".into(),
            },
            Order::Hold {
                unit: UnitRef {
                    nation: "fra".into(),
                    unit_type: UnitType::Army,
                    region: "kie".into(),
                },
            },
        ];

        let main_handler = handler_for(GamePhase::SpringMain);
        let main_resolution = main_handler.resolve(&mut board, main_orders);
        assert!(
            main_resolution.has_dislodgements,
            "a 2-strength supported attack should dislodge the 1-strength holder"
        );

        let retreat_phase = main_handler.next_phase(GamePhase::SpringMain, &main_resolution);
        assert_eq!(
            retreat_phase,
            GamePhase::SpringRetreat,
            "a dislodgement should route to the retreat phase"
        );

        let retreat_orders = vec![Order::Retreat {
            unit: UnitRef {
                nation: "fra".into(),
                unit_type: UnitType::Army,
                region: "kie".into(),
            },
            target: "ruh".into(),
        }];

        let retreat_handler = handler_for(retreat_phase);
        let is_valid = retreat_handler.validate(&retreat_orders[0], &board);
        assert!(is_valid.is_ok(), "retreat order should validate");

        let retreat_resolution = retreat_handler.resolve(&mut board, retreat_orders);
        assert_eq!(retreat_resolution.results.len(), 1);
        assert!(
            retreat_resolution.results[0].succeeded,
            "retreating to an unblocked, non-dislodger-origin region should succeed"
        );

        let next_main_phase = retreat_handler.next_phase(retreat_phase, &retreat_resolution);
        assert_eq!(
            next_main_phase,
            GamePhase::AutumnMain,
            "retreat phase should always advance to the next main phase"
        );
    }

    #[test]
    fn build_phase_handler_generic_validate() {
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("AUS: A bud"), unit_pos("AUS: A tri")],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![build::to_initial_ownerships(geo::standard_map())],
        };

        let orders = vec![Order::Build {
            nation: "aus".into(),
            territory: "vie".into(),
            unit_type: UnitType::Army,
        }];

        let handler = handler_for(GamePhase::WinterBuild);
        let is_valid = handler.validate(&orders[0], &board);
        assert!(is_valid.is_ok(), "build order should validate");

        let resolution = handler.resolve(&mut board, orders);
        assert_eq!(resolution.results.len(), 1);
        assert!(
            resolution.results[0].succeeded,
            "AUS should be entitled to build in its own unoccupied home supply center"
        );

        let next_phase = handler.next_phase(GamePhase::WinterBuild, &resolution);
        assert_eq!(
            next_phase,
            GamePhase::SpringMain,
            "build phase should always advance to spring main"
        );
    }

    #[test]
    fn handler_for_next_phase_branches() {
        // Every GamePhase, crossed with both dislodgement outcomes, dispatched through
        // handler_for(). Confirms the factory produces a working handler for each phase_kind
        // and that PhaseHandler::next_phase's default delegation to GamePhase::next covers
        // every branch (including the phases where the flag is a no-op).
        let dislodged = Resolution {
            has_dislodgements: true,
            results: vec![],
            retreat_snapshot: None,
        };
        let clear = Resolution {
            has_dislodgements: false,
            results: vec![],
            retreat_snapshot: None,
        };

        let test_cases = [
            (GamePhase::SpringMain, &dislodged, GamePhase::SpringRetreat),
            (GamePhase::SpringMain, &clear, GamePhase::AutumnMain),
            (GamePhase::SpringRetreat, &dislodged, GamePhase::AutumnMain),
            (GamePhase::SpringRetreat, &clear, GamePhase::AutumnMain),
            (GamePhase::AutumnMain, &dislodged, GamePhase::AutumnRetreat),
            (GamePhase::AutumnMain, &clear, GamePhase::WinterBuild),
            (GamePhase::AutumnRetreat, &dislodged, GamePhase::WinterBuild),
            (GamePhase::AutumnRetreat, &clear, GamePhase::WinterBuild),
            (GamePhase::WinterBuild, &dislodged, GamePhase::SpringMain),
            (GamePhase::WinterBuild, &clear, GamePhase::SpringMain),
        ];

        for (phase, resolution, expected) in test_cases {
            let handler = handler_for(phase);
            assert_eq!(
                handler.next_phase(phase, resolution),
                expected,
                "handler_for({phase:?}) next_phase mismatch for has_dislodgements={}",
                resolution.has_dislodgements
            );
        }
    }

    #[test]
    fn main_phase_hold_succeeds() {
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("ITA: A ven")],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![]
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
            pending_retreat: None,
            board_history: vec![]
        };

        let prev_units = board.units.clone();

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
        // Should be different showing move
        assert_ne!(board.units, prev_units);
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
            pending_retreat: None,
            board_history: vec![]
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
            pending_retreat: None,
            board_history: vec![]
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
    #[test]
    fn retreat_phase_test() {
        // GER attacks FRA's holding army in Kiel with support from Munich (strength 2 vs
        // 1), dislodging it. Kiel borders ber, mun, den, hol, ruh; ber is excluded as the
        // dislodger's origin and mun is excluded as still GER-occupied (it only supported,
        // it didn't move), leaving den/hol/ruh as legal retreat destinations. FRA retreats
        // to ruh, which nothing else in this turn touches.
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![
                unit_pos("GER: A ber"),
                unit_pos("GER: A mun"),
                unit_pos("FRA: A kie"),
            ],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![]
        };

        let main_orders = vec![
            Order::Move {
                unit: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "ber".into(),
                },
                target: "kie".into(),
            },
            Order::Support {
                unit: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "mun".into(),
                },
                supported: UnitRef {
                    nation: "ger".into(),
                    unit_type: UnitType::Army,
                    region: "ber".into(),
                },
                target: "kie".into(),
            },
            Order::Hold {
                unit: UnitRef {
                    nation: "fra".into(),
                    unit_type: UnitType::Army,
                    region: "kie".into(),
                },
            },
        ];

        let main_resolution = MainPhase.resolve(&mut board, main_orders);

        assert!(
            main_resolution.has_dislodgements,
            "a 2-strength supported attack should dislodge the 1-strength holder"
        );
        assert!(
            board.pending_retreat.is_some(),
            "MainPhase::resolve should stash a retreat snapshot on the board"
        );

        let retreat_orders = vec![Order::Retreat {
            unit: UnitRef {
                nation: "fra".into(),
                unit_type: UnitType::Army,
                region: "kie".into(),
            },
            target: "ruh".into(),
        }];

        let retreat_resolution = RetreatPhase.resolve(&mut board, retreat_orders);

        assert_eq!(retreat_resolution.results.len(), 1);
        assert!(
            retreat_resolution.results[0].succeeded,
            "retreating to an unblocked, non-dislodger-origin region should succeed"
        );
        assert!(
            !retreat_resolution.has_dislodgements,
            "retreats never cause further dislodgements"
        );
        assert!(
            board.pending_retreat.is_none(),
            "the snapshot should be consumed after RetreatPhase::resolve runs"
        );
    }

    #[test]
    fn build_phase_happy_path() {
        // AUS owns all 3 of its home supply centers (bud, tri, vie) per the standard
        // starting ownership map, but only has 2 units on the board, so it's entitled to
        // exactly one build. `board_history` holds the "last_time" ownership snapshot
        // BuildPhase::resolve needs (it pops the most recent one); using
        // `to_initial_ownerships` mirrors the "first winter" case the crate docs describe.
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("AUS: A bud"), unit_pos("AUS: A tri")],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![build::to_initial_ownerships(geo::standard_map())],
        };

        let orders = vec![Order::Build {
            nation: "aus".into(),
            territory: "vie".into(),
            unit_type: UnitType::Army,
        }];

        let resolution = BuildPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 1);
        assert!(
            resolution.results[0].succeeded,
            "AUS should be entitled to build in its own unoccupied home supply center"
        );
        assert!(!resolution.has_dislodgements, "builds never cause dislodgements");
    }

    #[test]
    fn build_phase_rejects_foreign_controlled_home_center() {
        // GER's unit is standing in Vienna, one of AUS's home supply centers (as if GER
        // captured it in an earlier turn). AUS is still in Build mode overall (it owns
        // bud + tri, giving it more SCs than its 1 unit), but a build order into vie must
        // be rejected: vie is still statically an AUS home center, but GER controls it
        // right now.
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("AUS: A bud"), unit_pos("GER: A vie")],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![build::to_initial_ownerships(geo::standard_map())],
        };

        let orders = vec![Order::Build {
            nation: "aus".into(),
            territory: "vie".into(),
            unit_type: UnitType::Army,
        }];

        let resolution = BuildPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 1);
        assert!(
            !resolution.results[0].succeeded,
            "AUS can't build in vie while GER's unit currently controls it"
        );
    }

    #[test]
    fn build_phase_disband_succeeds_when_over_unit_cap() {
        // AUS holds all 3 of its home centers (bud, tri, vie) but has a 4th unit sitting
        // in boh (not a supply center), putting it one unit over what its 3 centers
        // support -> Disband mode with an allowance of 1.
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![
                unit_pos("AUS: A bud"),
                unit_pos("AUS: A tri"),
                unit_pos("AUS: A vie"),
                unit_pos("AUS: A boh"),
            ],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![build::to_initial_ownerships(geo::standard_map())],
        };

        let orders = vec![Order::Disband {
            unit: UnitRef {
                nation: "aus".into(),
                unit_type: UnitType::Army,
                region: "boh".into(),
            },
        }];

        let resolution = BuildPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 1);
        assert!(
            resolution.results[0].succeeded,
            "AUS must disband down to its 3-center capacity, and boh is a legal target"
        );
    }

    #[test]
    fn build_phase_rejects_second_build_once_allowance_is_used() {
        // ENG owns all 3 of its home centers (edi, lon, lvp) but has only 2 units, so
        // it's entitled to exactly 1 build. Two build orders into the one empty center
        // (Army, then Fleet, so they're distinguishable orders rather than duplicates)
        // should succeed once and then fail with AllBuildsUsed.
        let mut board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("ENG: A lon"), unit_pos("ENG: F lvp")],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![build::to_initial_ownerships(geo::standard_map())],
        };

        let orders = vec![
            Order::Build {
                nation: "eng".into(),
                territory: "edi".into(),
                unit_type: UnitType::Army,
            },
            Order::Build {
                nation: "eng".into(),
                territory: "edi".into(),
                unit_type: UnitType::Fleet,
            },
        ];

        let resolution = BuildPhase.resolve(&mut board, orders);

        assert_eq!(resolution.results.len(), 2);

        let army_result = resolution
            .results
            .iter()
            .find(|r| r.unit_type == LibUnitType::Army)
            .expect("army build order should be present");
        let fleet_result = resolution
            .results
            .iter()
            .find(|r| r.unit_type == LibUnitType::Fleet)
            .expect("fleet build order should be present");

        assert!(
            army_result.succeeded,
            "the first build should use up ENG's only allowance"
        );
        assert!(
            !fleet_result.succeeded,
            "the second build should fail once the allowance is already used"
        );
    }
}