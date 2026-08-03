// core/src/board.rs
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use diplomacy::geo::{Map, ProvinceKey, RegionKey};
use diplomacy::judge::build::WorldState;
use diplomacy::{Nation, UnitPosition, UnitType};

use crate::phase::RetreatSnapshot;

pub struct Board {
    pub map: Arc<Map>,
    pub units: Vec<UnitPosition<'static, RegionKey>>,
    pub ownership: HashMap<ProvinceKey, Nation>,
    pub pending_retreat: Option<RetreatSnapshot>,
}

// For build-phase adjudication
impl WorldState for Board {
    fn nations(&self) -> HashSet<&Nation> {
        self.units.iter().map(|u| u.nation()).collect()
    }

    fn occupier(&self, province: &ProvinceKey) -> Option<&Nation> {
        self.ownership.get(province)
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