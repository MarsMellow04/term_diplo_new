// core/src/board.rs
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::todo;
use diplomacy::geo::{self, Map, ProvinceKey, RegionKey};
use diplomacy::judge::build::WorldState;
use diplomacy::{Nation, UnitPosition, UnitType};
use serde::{Deserialize, Serialize};

use crate::phase::{OrderResult, RetreatSnapshot};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BoardError {
    #[error("order is illegal in the current phase")]
    UnitMismatch,
}
pub struct Board {
    pub map: Arc<Map>,
    pub units: Vec<UnitPosition<'static, RegionKey>>,
    pub ownership: HashMap<ProvinceKey, Nation>,
    pub pending_retreat: Option<RetreatSnapshot>,
    pub board_history: Vec<HashMap<ProvinceKey, Nation>>
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardSnapshot {
    pub units: Vec<UnitPosition<'static, RegionKey>>,
    pub ownership: HashMap<ProvinceKey, Nation>,
    pub pending_retreat: Option<RetreatSnapshot>,
    pub board_history: Vec<HashMap<ProvinceKey, Nation>>,
}

impl Board {
    pub fn update_new_positions(&mut self, given_positions: Vec<UnitPosition<'static, RegionKey>>) -> Result<(), BoardError> {
        // TODO! If this has been updated from a main phase, the board will be missing currently dislodged positions
        // These are kept in the pending retreat until then and a check is done at the end to check all units are accounted for
        self.units = given_positions;
        Ok(())
    }

    pub fn units_for_nation(&self, nation: &Nation) -> HashSet<(UnitType, RegionKey)> {
        // Copy of private function
        self.units
            .iter()
            .filter(|u| u.nation() == nation)
            .map(|u| (u.unit.unit_type(), u.as_region_ref().region.clone()))
            .collect()
    }

    pub fn to_snapshot(&self) -> BoardSnapshot {
        BoardSnapshot {
            units: self.units.clone(),
            ownership: self.ownership.clone(),
            pending_retreat: self.pending_retreat.clone(),
            board_history: self.board_history.clone(),
        }
    }

    pub fn hydrate(snapshot: BoardSnapshot) -> Self {
        Board {
            map: Arc::new(geo::standard_map().clone()),
            units: snapshot.units,
            ownership: snapshot.ownership,
            pending_retreat: snapshot.pending_retreat,
            board_history: snapshot.board_history,
        }
    }
}

// For build-phase adjudication
impl WorldState for Board {
    fn nations(&self) -> HashSet<&Nation> {
        self.units.iter().map(|u| u.nation()).collect()
    }

    fn occupier(&self, province: &ProvinceKey) -> Option<&Nation> {
        self.units.iter().find(|u| u.region.province() == province).map(|u| u.nation())
    }

    fn unit_count(&self, nation: &Nation) -> u8 {
        self.units
            .iter()
            .filter(|u| u.nation() == nation)
            .count()
            .try_into()
            .unwrap()
    }

    fn units(&self, nation: &Nation) -> HashSet<(UnitType, RegionKey)> {
        self.units
            .iter()
            .filter(|u| u.nation() == nation)
            .map(|u| (u.unit.unit_type(), u.as_region_ref().region.clone()))
            .collect()
    }
}