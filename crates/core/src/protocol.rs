use uuid::Uuid;

use crate::{order::Order, phase::{GamePhase, Resolution}};
use serde::{Serialize, Deserialize};
use arrayvec::ArrayString;

#[derive(Serialize, Deserialize, Debug)]
pub enum Protocol {
    Auth(String),         
    Authenticated { session_id: Uuid },  // response, replaces "Ack{session_uuid}"
    Join(Uuid),                          // request
    Joined { nation: String },           // response, replaces "Ack{nation}"
    SubmitOrders(Vec<Order>),
    Ack,                                 // used only for SubmitOrders/SendMessage — nothing to return
    Event(GameEvent),
    Error(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum GameEvent {
    Disconnected,
    PlayerJoined(String),
    PlayerLeft(String),
    PhaseChanged(GamePhase),
    OrdersResolved(Resolution),
    NewMessage(ChatMessage),
    Error(String)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    sender: Uuid,
    recipient: Uuid,
    contents: ArrayString<1024>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::{Order, UnitRef, UnitType as WireUnitType};
    use crate::phase::{OrderResult, Resolution};
    use diplomacy::{Nation, UnitType as LibUnitType};
    use serde_json::json;

    /// Serializes `value`, asserts the JSON matches `expected` exactly (this is the wire
    /// format contract other implementations would need to match), then deserializes that
    /// JSON back and re-serializes it to prove the round-trip is lossless.
    fn assert_json_round_trip<T>(value: &T, expected: serde_json::Value)
    where
        T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug,
    {
        let actual = serde_json::to_value(value).expect("serialize");
        assert_eq!(actual, expected, "unexpected JSON shape for {value:?}");

        let deserialized: T = serde_json::from_value(actual.clone()).expect("deserialize");
        let re_serialized = serde_json::to_value(&deserialized).expect("re-serialize");
        assert_eq!(re_serialized, expected, "round-trip mismatch for {value:?}");
    }

    #[test]
    fn protocol_auth_json() {
        assert_json_round_trip(
            &Protocol::Auth("secret-token".into()),
            json!({ "Auth": "secret-token" }),
        );
    }

    #[test]
    fn protocol_authenticated_json() {
        // `Uuid`'s default serde impl encodes as a hyphenated string when human-readable
        // (JSON is) — same format `Protocol::Join` uses below.
        let session_id = Uuid::nil();
        assert_json_round_trip(
            &Protocol::Authenticated { session_id },
            json!({ "Authenticated": { "session_id": "00000000-0000-0000-0000-000000000000" } }),
        );
    }

    #[test]
    fn protocol_join_json() {
        let game_id = Uuid::nil();
        assert_json_round_trip(
            &Protocol::Join(game_id),
            json!({ "Join": "00000000-0000-0000-0000-000000000000" }),
        );
    }

    #[test]
    fn protocol_joined_json() {
        assert_json_round_trip(
            &Protocol::Joined { nation: "FRANCE".into() },
            json!({ "Joined": { "nation": "FRANCE" } }),
        );
    }

    #[test]
    fn protocol_submit_orders_json() {
        let orders = vec![
            Order::Hold {
                unit: UnitRef {
                    nation: "fra".into(),
                    unit_type: WireUnitType::Army,
                    region: "par".into(),
                },
            },
            Order::Move {
                unit: UnitRef {
                    nation: "fra".into(),
                    unit_type: WireUnitType::Army,
                    region: "par".into(),
                },
                target: "bur".into(),
            },
        ];

        assert_json_round_trip(
            &Protocol::SubmitOrders(orders),
            json!({
                "SubmitOrders": [
                    { "Hold": { "unit": { "nation": "fra", "unit_type": "Army", "region": "par" } } },
                    {
                        "Move": {
                            "unit": { "nation": "fra", "unit_type": "Army", "region": "par" },
                            "target": "bur"
                        }
                    }
                ]
            }),
        );
    }

    #[test]
    fn protocol_ack_json_is_a_bare_string() {
        // Unit variants have no payload, so serde's external tagging serializes them as a
        // plain JSON string rather than `{"Ack": null}`.
        assert_json_round_trip(&Protocol::Ack, json!("Ack"));
    }

    #[test]
    fn protocol_error_json() {
        assert_json_round_trip(
            &Protocol::Error("unknown session".into()),
            json!({ "Error": "unknown session" }),
        );
    }

    #[test]
    fn game_event_disconnected_json() {
        assert_json_round_trip(&Protocol::Event(GameEvent::Disconnected), json!({ "Event": "Disconnected" }));
    }

    #[test]
    fn game_event_player_joined_and_left_json() {
        assert_json_round_trip(
            &GameEvent::PlayerJoined("ENGLAND".into()),
            json!({ "PlayerJoined": "ENGLAND" }),
        );
        assert_json_round_trip(
            &GameEvent::PlayerLeft("ENGLAND".into()),
            json!({ "PlayerLeft": "ENGLAND" }),
        );
    }

    #[test]
    fn game_event_phase_changed_json() {
        assert_json_round_trip(
            &GameEvent::PhaseChanged(GamePhase::AutumnRetreat),
            json!({ "PhaseChanged": "AutumnRetreat" }),
        );
    }

    #[test]
    fn game_event_orders_resolved_json() {
        let resolution = Resolution {
            has_dislodgements: false,
            results: vec![OrderResult {
                nation: Nation::from("FRA"),
                unit_type: LibUnitType::Army,
                region: "par".into(),
                command: "FRA: A par HOLD".into(),
                succeeded: true,
            }],
            retreat_snapshot: None,
        };

        assert_json_round_trip(
            &GameEvent::OrdersResolved(resolution),
            json!({
                "OrdersResolved": {
                    "has_dislodgements": false,
                    "results": [
                        {
                            "nation": "FRA",
                            "unit_type": "A",
                            "region": "par",
                            "command": "FRA: A par HOLD",
                            "succeeded": true
                        }
                    ],
                    "retreat_snapshot": null
                }
            }),
        );
    }

    #[test]
    fn game_event_new_message_json() {
        let message = ChatMessage {
            sender: Uuid::nil(),
            recipient: Uuid::nil(),
            contents: "hello, diplomat".parse().unwrap(),
        };

        assert_json_round_trip(
            &GameEvent::NewMessage(message),
            json!({
                "NewMessage": {
                    "sender": "00000000-0000-0000-0000-000000000000",
                    "recipient": "00000000-0000-0000-0000-000000000000",
                    "contents": "hello, diplomat"
                }
            }),
        );
    }

    #[test]
    fn game_event_error_json() {
        assert_json_round_trip(
            &GameEvent::Error("adjudication failed".into()),
            json!({ "Error": "adjudication failed" }),
        );
    }

    #[test]
    fn protocol_event_nests_game_event_under_one_more_tag() {
        assert_json_round_trip(
            &Protocol::Event(GameEvent::PlayerJoined("GERMANY".into())),
            json!({ "Event": { "PlayerJoined": "GERMANY" } }),
        );
    }
}