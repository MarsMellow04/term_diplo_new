use std::collections::{HashMap, HashSet};

use diplomacy::{Nation, UnitType, geo::RegionKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    board::{Board, BoardSnapshot},
    phase::{GamePhase, Resolution, handler_for},
};


pub struct GameContext {
    pub game_id: Uuid,
    pub board: Board,
    pub phase: GamePhase,
    pub year: u16,
    pub turn_number: u32,
    pub players: HashMap<Uuid, Nation>,
}

#[derive(Error, Debug)]
pub enum ContextError {
    #[error("order is illegal in the current phase")]
    Illegal,
    #[error("order validation failed: {0}")]
    ValidationFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub board: BoardSnapshot,
    pub phase: GamePhase,
    pub year: u16,
    pub turn_number: u32,
    pub players: HashMap<Uuid, Nation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    pub game_id: Uuid,
    pub turn_number: u32,
    pub phase: GamePhase,
    pub game_state: GameSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl GameContext {
    pub fn player_units(&self, player:&Uuid) -> HashSet<(UnitType, RegionKey)> {
        let player_nation = self.players.get(player).expect("Player not in game");
        self.board.units_for_nation(player_nation)
    }

    pub fn advance_phase(&mut self, resolution: &Resolution) {
        let next = handler_for(self.phase).next_phase(self.phase, resolution);
        if self.phase == GamePhase::WinterBuild {
            self.year += 1;
        }
        self.phase = next;
        self.turn_number += 1;
    }

    /// Serializable snapshot of this context, ready to drop into `SnapshotRow::game_state`.
    pub fn to_snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            board: self.board.to_snapshot(),
            phase: self.phase,
            year: self.year,
            turn_number: self.turn_number,
            players: self.players.clone(),
        }
    }

    pub fn to_snapshot_row(&self) -> SnapshotRow {
        SnapshotRow {
            id: None,
            game_id: self.game_id,
            turn_number: self.turn_number,
            phase: self.phase,
            game_state: self.to_snapshot(),
            created_at: None,
        }
    }

    /// Rebuilds a live `GameContext` from a persisted snapshot. `game_id` is passed
    /// separately since it names *which* game this is, not part of its saved state.
    pub fn hydrate(game_id: Uuid, snapshot: GameSnapshot) -> Self {
        GameContext {
            game_id,
            board: Board::hydrate(snapshot.board),
            phase: snapshot.phase,
            year: snapshot.year,
            turn_number: snapshot.turn_number,
            players: snapshot.players,
        }
    }

    /// Rebuilds directly from a `snapshots` table row, e.g. the response body of
    /// `GET /rest/v1/snapshots?game_id=eq...&order=turn_number.desc&limit=1`.
    pub fn hydrate_from_row(row: SnapshotRow) -> Self {
        Self::hydrate(row.game_id, row.game_state)
    }
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use diplomacy::UnitPosition;
    use diplomacy::geo;
    use diplomacy::judge::build;

    use crate::order::{Order, UnitRef, UnitType as WireUnitType};
    use crate::phase::handler_for;

    fn unit_pos(s: &'static str) -> UnitPosition<'static, RegionKey> {
        s.parse().unwrap()
    }

    fn sample_context() -> GameContext {
        let board = Board {
            map: Arc::new(geo::standard_map().clone()),
            units: vec![unit_pos("ITA: A ven")],
            ownership: HashMap::new(),
            pending_retreat: None,
            board_history: vec![build::to_initial_ownerships(geo::standard_map())],
        };

        let mut players = HashMap::new();
        players.insert(Uuid::nil(), Nation::from("ITA"));

        GameContext {
            game_id: Uuid::nil(),
            board,
            phase: GamePhase::AutumnMain,
            year: 1901,
            turn_number: 3,
            players,
        }
    }

    #[test]
    fn advance_phase_rolls_over_year_only_out_of_winter_build() {
        let mut ctx = sample_context();

        let resolution = handler_for(ctx.phase).resolve(
            &mut ctx.board,
            vec![Order::Hold {
                unit: UnitRef {
                    nation: "ita".into(),
                    unit_type: WireUnitType::Army,
                    region: "ven".into(),
                },
            }],
        );
        ctx.advance_phase(&resolution);

        assert_eq!(ctx.phase, GamePhase::WinterBuild, "no dislodgements should skip Autumn Retreat");
        assert_eq!(ctx.turn_number, 4, "advancing always bumps turn_number");
        assert_eq!(ctx.year, 1901, "year only rolls over leaving WinterBuild, not entering it");

        let resolution = handler_for(ctx.phase).resolve(&mut ctx.board, vec![]);
        ctx.advance_phase(&resolution);

        assert_eq!(ctx.phase, GamePhase::SpringMain);
        assert_eq!(ctx.turn_number, 5);
        assert_eq!(ctx.year, 1902, "WinterBuild -> SpringMain is the year boundary");
    }

    #[test]
    fn save_and_hydrate_round_trips_through_json() {
        let ctx = sample_context();

        let row = ctx.to_snapshot_row();
        let json = serde_json::to_string(&row).expect("serialize snapshot row");
        let row_from_json: SnapshotRow = serde_json::from_str(&json).expect("deserialize snapshot row");

        let hydrated = GameContext::hydrate_from_row(row_from_json);

        assert_eq!(hydrated.game_id, ctx.game_id);
        assert_eq!(hydrated.phase, ctx.phase);
        assert_eq!(hydrated.year, ctx.year);
        assert_eq!(hydrated.turn_number, ctx.turn_number);
        assert_eq!(hydrated.players, ctx.players);
        assert_eq!(
            hydrated.board.units_for_nation(&Nation::from("ITA")),
            ctx.board.units_for_nation(&Nation::from("ITA")),
            "units should survive the save/hydrate round trip"
        );
    }
}