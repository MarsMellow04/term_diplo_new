use serde::{Deserialize, Serialize};
use std::fmt::{self, write};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Order {
    Move { unit: UnitRef, target: String },
    Support { unit: UnitRef, supported: UnitRef, target: String },
    Convoy { unit: UnitRef, army: UnitRef, target: String },
    Hold { unit: UnitRef },
    Retreat { unit: UnitRef, target: String },
    Disband { unit: UnitRef },
    Build { nation: String, territory: String, unit_type: UnitType },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnitRef {
    pub nation: String,
    pub unit_type: UnitType,
    pub region: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitType {
    Army,
    Fleet,
}

impl fmt::Display for UnitType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Army => write!(f, "Army"),
            Self::Fleet => write!(f, "Fleet"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PlayerOrder {
    Move { nation: String, unit_type: UnitType, region: String, target: String },
    Hold { nation: String, unit_type: UnitType, region: String },
    Support { /* ... */ },
    Convoy { /* ... */ },
    Retreat { /* ... */ },
    Disband { /* ... */ },
    Build { /* ... */ },
}