use std::str::FromStr;

use diplomacy::{
    Nation, UnitType as LibUnitType, geo::RegionKey, judge::{OrderState, Rulebook, Submission}, order::{ConvoyedMove, MainCommand, MoveCommand, Order as LibOrder, SupportedOrder},
};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct Resolution {
/// Resolution has been made to stop the stupid Lifetime issues. 
/// The command should be able to immediatley parse the Order Result into a LibOrder when needed, 
/// TODO!: Move this order reuslt over and a way to convert them.
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

        let submission = Submission::new(&*board.map, &board.units, lib_orders);

        // Outcome borrows submission — extract everything before returning
        let outcome = submission.adjudicate(Rulebook::default());

        // TODO! Figure out what to do when dislodged. 
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

    // Phase Handler Resolve tests

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