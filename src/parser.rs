use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use serde::Deserialize;

use crate::card::Card;
use crate::config::BlindRemap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Position {
    EP,
    MP,
    CO,
    BTN,
    SB,
    BB,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Street {
    Preflop = 0,
    Flop = 1,
    Turn = 2,
    River = 3,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionType {
    SmallBlind,
    BigBlind,
    Ante,
    Straddle,
    DeadBlind,
    Fold,
    Check,
    Call,
    Bet,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub seat: u8,
    pub kind: ActionType,
    pub amount: f64,
    pub all_in: bool,
}

#[derive(Clone, Debug)]
pub struct StreetData {
    pub street: Street,
    pub new_cards: Vec<Card>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct Winner {
    pub seat: u8,
    pub amount: f64,
    pub cards: Option<Vec<Card>>,
    pub hand_description: Option<String>,
    pub run: u8,
}

#[derive(Clone, Debug)]
pub struct PlayerInHand {
    pub id: String,
    pub seat: u8,
    pub name: String,
    pub stack: f64,
    pub hole_cards: Option<Vec<Card>>,
    pub position: Position,
}

#[derive(Clone, Debug)]
pub struct Hand {
    pub id: String,
    pub number: u32,
    pub small_blind: f64,
    pub big_blind: f64,
    pub bomb_pot: bool,
    pub players: Vec<PlayerInHand>,
    pub streets: Vec<StreetData>,
    pub winners: Vec<Winner>,
    pub real_showdown: bool,
    pub shown_cards: HashMap<u8, Vec<Card>>,
    pub uncalled_returns: HashMap<u8, f64>,
    pub run_it_twice: bool,
    pub run2_cards: Vec<(Street, Vec<Card>)>,
}

pub struct GameData {
    pub hands: Vec<Hand>,
    pub player_names: HashMap<String, String>,
}

// --- serde raw types ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGameFile {
    hands: Vec<RawHand>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHand {
    id: String,
    number: String,
    game_type: String,
    small_blind: f64,
    big_blind: f64,
    #[allow(dead_code)]
    ante: Option<f64>,
    #[allow(dead_code)]
    straddle_seat: Option<u8>,
    dealer_seat: u8,
    bomb_pot: bool,
    players: Vec<RawPlayer>,
    events: Vec<RawEvent>,
}

#[derive(Deserialize)]
struct RawPlayer {
    id: String,
    seat: u8,
    name: String,
    stack: f64,
    hand: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RawEvent {
    payload: RawPayload,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPayload {
    #[serde(rename = "type")]
    event_type: u8,
    seat: Option<u8>,
    value: Option<f64>,
    all_in: Option<bool>,
    cards: Option<Vec<Option<String>>>,
    turn: Option<u8>,
    run: Option<u8>,
    #[allow(dead_code)]
    pot: Option<f64>,
    #[allow(dead_code)]
    position: Option<u8>,
    hand_description: Option<String>,
    run_number: Option<String>,
    #[allow(dead_code)]
    bomb_pot: Option<bool>,
}

pub fn parse_files<S: std::hash::BuildHasher>(
    paths: &[String],
    unify: &HashMap<String, String, S>,
    blind_remap: &[BlindRemap],
) -> Result<GameData, Box<dyn std::error::Error>> {
    let mut all_hands = Vec::new();
    let mut player_names: HashMap<String, String> = HashMap::new();

    let id_unify = build_id_unify_map(paths, unify)?;

    for path in paths {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let raw: RawGameFile = serde_json::from_reader(reader)?;

        for raw_hand in raw.hands {
            for rp in &raw_hand.players {
                let canonical = id_unify.get(&rp.id).unwrap_or(&rp.id);
                player_names.entry(canonical.clone()).or_insert_with(|| rp.name.clone());
            }

            if let Some(hand) = process_hand(&raw_hand, &id_unify, blind_remap) {
                all_hands.push(hand);
            }
        }
    }

    Ok(GameData {
        hands: all_hands,
        player_names,
    })
}

fn build_id_unify_map<S: std::hash::BuildHasher>(
    paths: &[String],
    unify: &HashMap<String, String, S>,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut name_to_ids: HashMap<String, Vec<String>> = HashMap::new();

    for path in paths {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let raw: RawGameFile = serde_json::from_reader(reader)?;
        for raw_hand in &raw.hands {
            for rp in &raw_hand.players {
                let ids = name_to_ids.entry(rp.name.clone()).or_default();
                if !ids.contains(&rp.id) {
                    ids.push(rp.id.clone());
                }
            }
        }
    }

    let mut id_map: HashMap<String, String> = HashMap::new();

    // Auto-unify: same name, multiple IDs → map all to first-seen ID.
    for ids in name_to_ids.values() {
        if ids.len() > 1 {
            let primary = &ids[0];
            for id in ids.iter().skip(1) {
                id_map.insert(id.clone(), primary.clone());
            }
        }
    }

    // Config aliases: group by canonical key, merge all alias IDs to one primary.
    if !unify.is_empty() {
        let mut canonical_to_aliases: HashMap<&str, Vec<&str>> = HashMap::new();
        for (alias_name, canonical_name) in unify {
            canonical_to_aliases
                .entry(canonical_name.as_str())
                .or_default()
                .push(alias_name.as_str());
        }

        for (canonical_name, alias_names) in &canonical_to_aliases {
            let all_names = std::iter::once(*canonical_name).chain(alias_names.iter().copied());

            // Resolve through any auto-unify mapping to find the true primary.
            let primary_id = all_names
                .clone()
                .flat_map(|name| name_to_ids.get(name).into_iter().flatten())
                .next()
                .map(|id| id_map.get(id).unwrap_or(id).clone());

            let Some(primary_id) = primary_id else {
                continue;
            };

            for name in all_names {
                if let Some(ids) = name_to_ids.get(name) {
                    for id in ids {
                        let resolved = id_map.get(id).unwrap_or(id);
                        if resolved != &primary_id {
                            id_map.insert(id.clone(), primary_id.clone());
                        }
                    }
                }
            }
        }
    }

    Ok(id_map)
}

fn apply_blind_remap(sb: f64, bb: f64, rules: &[BlindRemap]) -> (f64, f64) {
    for rule in rules {
        if (rule.from[0] - sb).abs() < f64::EPSILON && (rule.from[1] - bb).abs() < f64::EPSILON {
            return (rule.to[0], rule.to[1]);
        }
    }
    (sb, bb)
}

fn process_hand(
    raw: &RawHand,
    id_unify: &HashMap<String, String>,
    blind_remap: &[BlindRemap],
) -> Option<Hand> {
    if raw.game_type != "th" {
        return None;
    }
    if raw.bomb_pot {
        return None;
    }
    if is_double_board_game(&raw.events) {
        return None;
    }

    let run_it_twice = has_multiple_runs(&raw.events);

    let number: u32 = raw.number.parse().unwrap_or(0);
    let (small_blind, big_blind) = apply_blind_remap(raw.small_blind, raw.big_blind, blind_remap);
    let mut seats_sorted: Vec<u8> = raw.players.iter().map(|p| p.seat).collect();
    seats_sorted.sort_unstable();
    let positions = assign_positions(&seats_sorted, raw.dealer_seat);

    let mut shown_cards: HashMap<u8, Vec<Card>> = HashMap::new();

    let mut players: Vec<PlayerInHand> = raw
        .players
        .iter()
        .map(|rp| {
            let id = id_unify.get(&rp.id).unwrap_or(&rp.id).clone();
            let hole = rp.hand.as_ref().and_then(|h| parse_card_strings(h));
            if let Some(ref cards) = hole {
                shown_cards.insert(rp.seat, cards.clone());
            }
            let pos = positions.get(&rp.seat).copied().unwrap_or(Position::EP);
            PlayerInHand {
                id,
                seat: rp.seat,
                name: rp.name.clone(),
                stack: rp.stack,
                hole_cards: hole,
                position: pos,
            }
        })
        .collect();

    let ProcessedEvents {
        mut streets,
        winners,
        uncalled_returns,
        show_count,
        run2_cards,
    } = process_events(&raw.events, &mut shown_cards);

    remove_spurious_folds(&raw.events, &mut streets);

    for p in &mut players {
        if p.hole_cards.is_none()
            && let Some(cards) = shown_cards.get(&p.seat)
        {
            p.hole_cards = Some(cards.clone());
        }
    }

    let real_showdown = show_count >= 2;

    Some(Hand {
        id: raw.id.clone(),
        number,
        small_blind,
        big_blind,
        bomb_pot: raw.bomb_pot,
        players,
        streets,
        winners,
        real_showdown,
        shown_cards,
        uncalled_returns,
        run_it_twice,
        run2_cards,
    })
}

fn has_multiple_runs(events: &[RawEvent]) -> bool {
    events.iter().any(|ev| ev.payload.event_type == 9 && ev.payload.run.unwrap_or(1) > 1)
}

fn has_rit_vote(events: &[RawEvent]) -> bool {
    events.iter().any(|ev| ev.payload.event_type == 14)
}

/// Double-board games deal two boards from the start (game format).
/// Run-it-twice is a player choice after all-in, signaled by a type 14 vote.
fn is_double_board_game(events: &[RawEvent]) -> bool {
    has_multiple_runs(events) && !has_rit_vote(events)
}

fn assign_positions(seats_sorted: &[u8], dealer_seat: u8) -> HashMap<u8, Position> {
    let n = seats_sorted.len();
    if n == 0 {
        return HashMap::new();
    }

    let dealer_idx = seats_sorted.iter().position(|&s| s == dealer_seat).unwrap_or(0);

    let templates: &[&[Position]] = &[
        &[],
        &[Position::BTN],
        &[Position::BTN, Position::BB],
        &[Position::BTN, Position::SB, Position::BB],
        &[Position::BTN, Position::SB, Position::BB, Position::EP],
        &[Position::BTN, Position::SB, Position::BB, Position::EP, Position::CO],
        &[Position::BTN, Position::SB, Position::BB, Position::EP, Position::MP, Position::CO],
        &[
            Position::BTN,
            Position::SB,
            Position::BB,
            Position::EP,
            Position::EP,
            Position::MP,
            Position::CO,
        ],
        &[
            Position::BTN,
            Position::SB,
            Position::BB,
            Position::EP,
            Position::EP,
            Position::MP,
            Position::MP,
            Position::CO,
        ],
        &[
            Position::BTN,
            Position::SB,
            Position::BB,
            Position::EP,
            Position::EP,
            Position::EP,
            Position::MP,
            Position::MP,
            Position::CO,
        ],
        &[
            Position::BTN,
            Position::SB,
            Position::BB,
            Position::EP,
            Position::EP,
            Position::EP,
            Position::MP,
            Position::MP,
            Position::MP,
            Position::CO,
        ],
    ];

    let template = if n < templates.len() { templates[n] } else { templates[templates.len() - 1] };

    let mut map = HashMap::with_capacity(n);
    for i in 0..n {
        let seat = seats_sorted[(dealer_idx + i) % n];
        let pos = if i < template.len() { template[i] } else { Position::EP };
        map.insert(seat, pos);
    }
    map
}

struct ProcessedEvents {
    streets: Vec<StreetData>,
    winners: Vec<Winner>,
    uncalled_returns: HashMap<u8, f64>,
    show_count: usize,
    run2_cards: Vec<(Street, Vec<Card>)>,
}

fn process_events(
    events: &[RawEvent],
    shown_cards: &mut HashMap<u8, Vec<Card>>,
) -> ProcessedEvents {
    let mut streets = vec![StreetData {
        street: Street::Preflop,
        new_cards: Vec::new(),
        actions: Vec::new(),
    }];
    let mut winners = Vec::new();
    let mut uncalled_returns: HashMap<u8, f64> = HashMap::new();
    let mut show_count: usize = 0;
    let mut run2_cards: Vec<(Street, Vec<Card>)> = Vec::new();

    for ev in events {
        let p = &ev.payload;

        if p.event_type == 9 {
            let street = match p.turn.unwrap_or(0) {
                1 => Street::Flop,
                2 => Street::Turn,
                3 => Street::River,
                _ => continue,
            };
            let new_cards =
                p.cards.as_ref().map(|cs| parse_optional_card_vec(cs)).unwrap_or_default();

            if p.run.unwrap_or(1) > 1 {
                run2_cards.push((street, new_cards));
                continue;
            }
            streets.push(StreetData {
                street,
                new_cards,
                actions: Vec::new(),
            });
            continue;
        }

        if let Some(kind) = map_kind(p.event_type) {
            let Some(seat) = p.seat else { continue };
            let action = Action {
                seat,
                kind,
                amount: p.value.unwrap_or(0.0),
                all_in: p.all_in.unwrap_or(false),
            };
            if let Some(current) = streets.last_mut() {
                current.actions.push(action);
            }
            continue;
        }

        match p.event_type {
            10 => {
                let Some(seat) = p.seat else { continue };
                let cards = p.cards.as_ref().and_then(|cs| {
                    let parsed = parse_optional_card_vec(cs);
                    if parsed.is_empty() { None } else { Some(parsed) }
                });
                if let Some(ref c) = cards {
                    shown_cards.entry(seat).or_insert_with(|| c.clone());
                }
                let run: u8 = p.run_number.as_deref().and_then(|s| s.parse().ok()).unwrap_or(1);
                winners.push(Winner {
                    seat,
                    amount: p.value.unwrap_or(0.0),
                    cards,
                    hand_description: p.hand_description.clone(),
                    run,
                });
            }
            12 => {
                show_count += 1;
                if let Some(seat) = p.seat
                    && let Some(ref card_strs) = p.cards
                {
                    let parsed = parse_optional_card_vec(card_strs);
                    if !parsed.is_empty() {
                        shown_cards.insert(seat, parsed);
                    }
                }
            }
            16 => {
                if let (Some(seat), Some(val)) = (p.seat, p.value) {
                    *uncalled_returns.entry(seat).or_insert(0.0) += val;
                }
            }
            _ => {}
        }
    }

    ProcessedEvents {
        streets,
        winners,
        uncalled_returns,
        show_count,
        run2_cards,
    }
}

fn map_kind(event_type: u8) -> Option<ActionType> {
    match event_type {
        0 => Some(ActionType::Fold),
        1 => Some(ActionType::Check),
        2 => Some(ActionType::BigBlind),
        3 => Some(ActionType::SmallBlind),
        4 => Some(ActionType::Ante),
        5 => Some(ActionType::Straddle),
        6 => Some(ActionType::DeadBlind),
        7 => Some(ActionType::Call),
        8 => Some(ActionType::Bet),
        _ => None,
    }
}

fn remove_spurious_folds(events: &[RawEvent], streets: &mut [StreetData]) {
    let mut last_fold_idx: HashMap<u8, usize> = HashMap::new();
    let mut has_later_action: HashMap<u8, bool> = HashMap::new();

    for (i, ev) in events.iter().enumerate() {
        let p = &ev.payload;
        let Some(seat) = p.seat else { continue };
        match p.event_type {
            0 => {
                last_fold_idx.insert(seat, i);
            }
            // type 12 (SHOW) excluded: a player can fold then show cards
            1 | 7 | 8 | 10 => {
                if let Some(&fold_i) = last_fold_idx.get(&seat)
                    && i > fold_i
                {
                    has_later_action.insert(seat, true);
                }
            }
            _ => {}
        }
    }

    let spurious_seats: Vec<u8> = has_later_action.keys().copied().collect();

    for sd in streets.iter_mut() {
        sd.actions.retain(|a| !(a.kind == ActionType::Fold && spurious_seats.contains(&a.seat)));
    }
}

fn parse_optional_card_vec(cards: &[Option<String>]) -> Vec<Card> {
    cards.iter().filter_map(|opt| opt.as_ref().and_then(|s| Card::parse(s))).collect()
}

fn parse_card_strings(cards: &[String]) -> Option<Vec<Card>> {
    let parsed: Vec<Card> = cards.iter().filter_map(|s| Card::parse(s)).collect();
    if parsed.is_empty() { None } else { Some(parsed) }
}

pub fn invested(hand: &Hand, seat: u8) -> f64 {
    let mut additive_total = 0.0_f64;
    let mut street_maxes = [0.0_f64; 4];
    for (i, sd) in hand.streets.iter().enumerate() {
        for a in &sd.actions {
            if a.seat == seat && is_monetary(a.kind) {
                if matches!(a.kind, ActionType::Ante | ActionType::DeadBlind) {
                    additive_total += a.amount;
                } else {
                    street_maxes[i] = street_maxes[i].max(a.amount);
                }
            }
        }
    }
    additive_total + street_maxes.iter().sum::<f64>()
}

pub fn net_profit(hand: &Hand, seat: u8) -> f64 {
    let cost = invested(hand, seat);
    let won: f64 = hand.winners.iter().filter(|w| w.seat == seat).map(|w| w.amount).sum();
    let returned = hand.uncalled_returns.get(&seat).copied().unwrap_or(0.0);
    won + returned - cost
}

pub fn is_monetary(at: ActionType) -> bool {
    matches!(
        at,
        ActionType::SmallBlind
            | ActionType::BigBlind
            | ActionType::Ante
            | ActionType::Straddle
            | ActionType::DeadBlind
            | ActionType::Call
            | ActionType::Bet
    )
}

#[cfg(test)]
#[allow(clippy::type_complexity, clippy::needless_pass_by_value)]
pub mod test_helpers {
    use super::*;
    use std::collections::HashMap;

    pub struct HandBuilder {
        id: String,
        number: u32,
        game_type: String,
        small_blind: f64,
        big_blind: f64,
        dealer_seat: u8,
        bomb_pot: bool,
        players: Vec<(String, u8, String, f64, Option<Vec<String>>)>,
        events: Vec<serde_json::Value>,
    }

    impl Default for HandBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl HandBuilder {
        pub fn new() -> Self {
            Self {
                id: "test_hand".into(),
                number: 1,
                game_type: "th".into(),
                small_blind: 0.5,
                big_blind: 1.0,
                dealer_seat: 1,
                bomb_pot: false,
                players: Vec::new(),
                events: Vec::new(),
            }
        }

        pub fn id(mut self, id: &str) -> Self {
            self.id = id.into();
            self
        }

        pub fn number(mut self, n: u32) -> Self {
            self.number = n;
            self
        }

        pub fn game_type(mut self, gt: &str) -> Self {
            self.game_type = gt.into();
            self
        }

        pub fn blinds(mut self, sb: f64, bb: f64) -> Self {
            self.small_blind = sb;
            self.big_blind = bb;
            self
        }

        pub fn dealer(mut self, seat: u8) -> Self {
            self.dealer_seat = seat;
            self
        }

        pub fn bomb_pot(mut self) -> Self {
            self.bomb_pot = true;
            self
        }

        pub fn player(mut self, id: &str, seat: u8, name: &str, stack: f64) -> Self {
            self.players.push((id.into(), seat, name.into(), stack, None));
            self
        }

        pub fn player_with_hand(
            mut self,
            id: &str,
            seat: u8,
            name: &str,
            stack: f64,
            cards: &[&str],
        ) -> Self {
            self.players.push((
                id.into(),
                seat,
                name.into(),
                stack,
                Some(cards.iter().map(|s| (*s).to_string()).collect()),
            ));
            self
        }

        pub fn event(mut self, payload: serde_json::Value) -> Self {
            self.events.push(serde_json::json!({ "payload": payload }));
            self
        }

        pub fn sb(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 3, "seat": seat, "value": value}))
        }

        pub fn bb(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 2, "seat": seat, "value": value}))
        }

        pub fn ante(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 4, "seat": seat, "value": value}))
        }

        pub fn fold(self, seat: u8) -> Self {
            self.event(serde_json::json!({"type": 0, "seat": seat}))
        }

        pub fn check(self, seat: u8) -> Self {
            self.event(serde_json::json!({"type": 1, "seat": seat}))
        }

        pub fn call(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 7, "seat": seat, "value": value}))
        }

        pub fn call_all_in(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 7, "seat": seat, "value": value, "allIn": true}))
        }

        pub fn bet(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 8, "seat": seat, "value": value}))
        }

        pub fn bet_all_in(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 8, "seat": seat, "value": value, "allIn": true}))
        }

        pub fn flop(self, cards: &[&str]) -> Self {
            let cs: Vec<serde_json::Value> =
                cards.iter().map(|s| serde_json::Value::String(s.to_string())).collect();
            self.event(serde_json::json!({"type": 9, "turn": 1, "run": 1, "cards": cs}))
        }

        pub fn turn(self, card: &str) -> Self {
            self.event(serde_json::json!({"type": 9, "turn": 2, "run": 1, "cards": [card]}))
        }

        pub fn river(self, card: &str) -> Self {
            self.event(serde_json::json!({"type": 9, "turn": 3, "run": 1, "cards": [card]}))
        }

        pub fn board_run2(self, turn_val: u8, cards: &[&str]) -> Self {
            let cs: Vec<serde_json::Value> =
                cards.iter().map(|s| serde_json::Value::String(s.to_string())).collect();
            self.event(serde_json::json!({"type": 9, "turn": turn_val, "run": 2, "cards": cs}))
        }

        pub fn win(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 10, "seat": seat, "value": value}))
        }

        pub fn win_run(self, seat: u8, value: f64, run: u8) -> Self {
            self.event(
                serde_json::json!({"type": 10, "seat": seat, "value": value, "runNumber": run.to_string()}),
            )
        }

        pub fn win_with_cards(self, seat: u8, value: f64, cards: &[&str]) -> Self {
            let cs: Vec<serde_json::Value> =
                cards.iter().map(|s| serde_json::Value::String(s.to_string())).collect();
            self.event(serde_json::json!({"type": 10, "seat": seat, "value": value, "cards": cs}))
        }

        pub fn show(self, seat: u8, cards: &[&str]) -> Self {
            let cs: Vec<serde_json::Value> =
                cards.iter().map(|s| serde_json::Value::String(s.to_string())).collect();
            self.event(serde_json::json!({"type": 12, "seat": seat, "cards": cs}))
        }

        pub fn rit_vote(self) -> Self {
            self.event(serde_json::json!({"type": 14, "approved": true, "approvedSeats": []}))
        }

        pub fn showdown(self) -> Self {
            self.event(serde_json::json!({"type": 15}))
        }

        pub fn uncalled_return(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 16, "seat": seat, "value": value}))
        }

        pub fn dead_blind(self, seat: u8, value: f64) -> Self {
            self.event(serde_json::json!({"type": 6, "seat": seat, "value": value}))
        }

        pub fn build_json(&self) -> String {
            let players: Vec<serde_json::Value> = self
                .players
                .iter()
                .map(|(id, seat, name, stack, hand)| {
                    let mut p = serde_json::json!({
                        "id": id,
                        "seat": seat,
                        "name": name,
                        "stack": stack,
                    });
                    if let Some(h) = hand {
                        p["hand"] = serde_json::json!(h);
                    }
                    p
                })
                .collect();

            let hand = serde_json::json!({
                "id": self.id,
                "number": self.number.to_string(),
                "gameType": self.game_type,
                "smallBlind": self.small_blind,
                "bigBlind": self.big_blind,
                "ante": null,
                "straddleSeat": null,
                "dealerSeat": self.dealer_seat,
                "bombPot": self.bomb_pot,
                "players": players,
                "events": self.events,
            });

            serde_json::json!({
                "generatedAt": "2026-03-11T00:00:00.000Z",
                "playerId": "test",
                "gameId": "test_game",
                "hands": [hand],
            })
            .to_string()
        }

        pub fn build_multi_json(builders: &[&HandBuilder]) -> String {
            let hands: Vec<serde_json::Value> = builders
                .iter()
                .map(|b| {
                    let players: Vec<serde_json::Value> = b
                        .players
                        .iter()
                        .map(|(id, seat, name, stack, hand)| {
                            let mut p = serde_json::json!({
                                "id": id,
                                "seat": seat,
                                "name": name,
                                "stack": stack,
                            });
                            if let Some(h) = hand {
                                p["hand"] = serde_json::json!(h);
                            }
                            p
                        })
                        .collect();

                    serde_json::json!({
                        "id": b.id,
                        "number": b.number.to_string(),
                        "gameType": b.game_type,
                        "smallBlind": b.small_blind,
                        "bigBlind": b.big_blind,
                        "ante": null,
                        "straddleSeat": null,
                        "dealerSeat": b.dealer_seat,
                        "bombPot": b.bomb_pot,
                        "players": players,
                        "events": b.events,
                    })
                })
                .collect();

            serde_json::json!({
                "generatedAt": "2026-03-11T00:00:00.000Z",
                "playerId": "test",
                "gameId": "test_game",
                "hands": hands,
            })
            .to_string()
        }

        pub fn write_to_tmp(&self) -> tempfile::NamedTempFile {
            use std::io::Write;
            let mut f = tempfile::NamedTempFile::new().unwrap();
            f.write_all(self.build_json().as_bytes()).unwrap();
            f
        }

        pub fn write_multi_to_tmp(builders: &[&HandBuilder]) -> tempfile::NamedTempFile {
            use std::io::Write;
            let mut f = tempfile::NamedTempFile::new().unwrap();
            f.write_all(Self::build_multi_json(builders).as_bytes()).unwrap();
            f
        }
    }

    pub fn parse_single_hand(builder: &HandBuilder) -> Option<Hand> {
        let tmp = builder.write_to_tmp();
        let path = tmp.path().to_string_lossy().to_string();
        let data = parse_files(&[path], &HashMap::new(), &[]).unwrap();
        data.hands.into_iter().next()
    }

    pub fn parse_game_data(builder: &HandBuilder) -> GameData {
        let tmp = builder.write_to_tmp();
        let path = tmp.path().to_string_lossy().to_string();
        parse_files(&[path], &HashMap::new(), &[]).unwrap()
    }

    pub fn parse_multi_game_data(builders: &[&HandBuilder]) -> GameData {
        let tmp = HandBuilder::write_multi_to_tmp(builders);
        let path = tmp.path().to_string_lossy().to_string();
        parse_files(&[path], &HashMap::new(), &[]).unwrap()
    }

    pub fn parse_game_data_with_unify<S: std::hash::BuildHasher>(
        builder: &HandBuilder,
        unify: &HashMap<String, String, S>,
    ) -> GameData {
        let tmp = builder.write_to_tmp();
        let path = tmp.path().to_string_lossy().to_string();
        parse_files(&[path], unify, &[]).unwrap()
    }

    pub fn parse_multi_game_data_with_unify<S: std::hash::BuildHasher>(
        builders: &[&HandBuilder],
        unify: &HashMap<String, String, S>,
    ) -> GameData {
        let tmp = HandBuilder::write_multi_to_tmp(builders);
        let path = tmp.path().to_string_lossy().to_string();
        parse_files(&[path], unify, &[]).unwrap()
    }

    pub fn parse_game_data_with_remap(
        builder: &HandBuilder,
        blind_remap: &[crate::config::BlindRemap],
    ) -> GameData {
        let tmp = builder.write_to_tmp();
        let path = tmp.path().to_string_lossy().to_string();
        parse_files(&[path], &HashMap::new(), blind_remap).unwrap()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn parse_minimal_hand() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(1)
            .fold(2)
            .win(3, 1.5);

        let hand = parse_single_hand(&b).unwrap();
        assert_eq!(hand.id, "test_hand");
        assert_eq!(hand.number, 1);
        assert_eq!(hand.small_blind, 0.5);
        assert_eq!(hand.big_blind, 1.0);
        assert_eq!(hand.players.len(), 3);
        assert_eq!(hand.winners.len(), 1);
        assert_eq!(hand.winners[0].amount, 1.5);
    }

    #[test]
    fn parse_empty_hands_array() {
        let json = serde_json::json!({
            "generatedAt": "2026-03-11",
            "playerId": "test",
            "gameId": "test",
            "hands": [],
        })
        .to_string();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut &tmp, json.as_bytes()).unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        let data = parse_files(&[path], &std::collections::HashMap::new(), &[]).unwrap();
        assert!(data.hands.is_empty());
    }

    #[test]
    fn filter_omaha_hands() {
        let b = HandBuilder::new()
            .game_type("omaha")
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(1, 1.0)
            .win(1, 1.5);

        assert!(parse_single_hand(&b).is_none());
    }

    #[test]
    fn filter_bomb_pots() {
        let b = HandBuilder::new()
            .bomb_pot()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(1, 1.0)
            .win(1, 1.5);

        assert!(parse_single_hand(&b).is_none());
    }

    #[test]
    fn run_it_twice_kept() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(1, 1.0)
            .call(2, 1.0)
            .check(1)
            .flop(&["Ah", "Kd", "Qs"])
            .bet_all_in(1, 100.0)
            .call_all_in(2, 100.0)
            .rit_vote()
            .turn("Js")
            .board_run2(2, &["7h"])
            .river("Ts")
            .board_run2(3, &["2c"])
            .showdown()
            .show(1, &["As", "Kh"])
            .show(2, &["9s", "8s"])
            .win_run(1, 100.0, 1)
            .win_run(2, 100.0, 2);

        let hand = parse_single_hand(&b).unwrap();
        assert!(hand.run_it_twice);
        assert_eq!(hand.run2_cards.len(), 2);
        assert_eq!(hand.run2_cards[0].0, Street::Turn);
        assert_eq!(hand.run2_cards[1].0, Street::River);
        assert_eq!(hand.winners.len(), 2);
        assert_eq!(hand.winners[0].run, 1);
        assert_eq!(hand.winners[1].run, 2);
    }

    #[test]
    fn filter_double_board_game() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(1, 1.0)
            .call(2, 1.0)
            .check(1)
            .flop(&["Ah", "Kd", "Qs"])
            .board_run2(1, &["Jh", "Td", "9s"])
            .win(1, 2.0);

        assert!(parse_single_hand(&b).is_none());
    }

    #[test]
    fn position_assignment_6_players() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .player("p4", 4, "Diana", 100.0)
            .player("p5", 5, "Eve", 100.0)
            .player("p6", 6, "Frank", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(4)
            .fold(5)
            .fold(6)
            .fold(1)
            .win(3, 1.5);

        let hand = parse_single_hand(&b).unwrap();
        let pos_map: std::collections::HashMap<u8, Position> =
            hand.players.iter().map(|p| (p.seat, p.position)).collect();
        assert_eq!(pos_map[&1], Position::BTN);
        assert_eq!(pos_map[&2], Position::SB);
        assert_eq!(pos_map[&3], Position::BB);
        assert_eq!(pos_map[&4], Position::EP);
        assert_eq!(pos_map[&5], Position::MP);
        assert_eq!(pos_map[&6], Position::CO);
    }

    #[test]
    fn position_assignment_3_players() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 3, "Bob", 100.0)
            .player("p3", 7, "Charlie", 100.0)
            .dealer(3)
            .sb(7, 0.5)
            .bb(1, 1.0)
            .fold(3)
            .fold(7)
            .win(1, 1.5);

        let hand = parse_single_hand(&b).unwrap();
        let pos_map: std::collections::HashMap<u8, Position> =
            hand.players.iter().map(|p| (p.seat, p.position)).collect();
        assert_eq!(pos_map[&3], Position::BTN);
        assert_eq!(pos_map[&7], Position::SB);
        assert_eq!(pos_map[&1], Position::BB);
    }

    #[test]
    fn position_assignment_2_players() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 5, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(5, 1.0)
            .fold(1)
            .win(5, 1.5);

        let hand = parse_single_hand(&b).unwrap();
        let pos_map: std::collections::HashMap<u8, Position> =
            hand.players.iter().map(|p| (p.seat, p.position)).collect();
        assert_eq!(pos_map[&1], Position::BTN);
        assert_eq!(pos_map[&5], Position::BB);
    }

    #[test]
    fn spurious_fold_removal() {
        // Seat 2 folds then checks later — spurious fold should be removed
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(2) // spurious fold
            .call(2, 1.0) // acts after fold — proves fold was spurious
            .call(1, 1.0)
            .check(3)
            .flop(&["Ah", "Kd", "Qs"])
            .check(2)
            .check(3)
            .check(1)
            .win(1, 3.0);

        let hand = parse_single_hand(&b).unwrap();
        let preflop = &hand.streets[0];
        let fold_count =
            preflop.actions.iter().filter(|a| a.kind == ActionType::Fold && a.seat == 2).count();
        assert_eq!(fold_count, 0, "spurious fold should be removed");
    }

    #[test]
    fn legitimate_fold_kept() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(1) // real fold — no later action
            .call(2, 1.0)
            .check(3)
            .flop(&["Ah", "Kd", "Qs"])
            .check(2)
            .check(3)
            .win(2, 2.5);

        let hand = parse_single_hand(&b).unwrap();
        let preflop = &hand.streets[0];
        let fold_count =
            preflop.actions.iter().filter(|a| a.kind == ActionType::Fold && a.seat == 1).count();
        assert_eq!(fold_count, 1, "legitimate fold should be kept");
    }

    #[test]
    fn fold_then_show_is_not_spurious() {
        // Type 12 (show) should NOT trigger spurious fold removal
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .show(1, &["As", "Kd"])
            .win(2, 1.5);

        let hand = parse_single_hand(&b).unwrap();
        let fold_count = hand.streets[0]
            .actions
            .iter()
            .filter(|a| a.kind == ActionType::Fold && a.seat == 1)
            .count();
        assert_eq!(fold_count, 1, "fold then show is legitimate");
    }

    #[test]
    fn net_profit_simple_win() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .win(1, 2.0);

        let hand = parse_single_hand(&b).unwrap();
        let profit_p1 = net_profit(&hand, 1);
        let profit_p2 = net_profit(&hand, 2);
        assert!((profit_p1 - 1.0).abs() < 0.001);
        assert!((profit_p2 - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn net_profit_with_uncalled_return() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet(1, 3.0)
            .fold(2)
            .uncalled_return(1, 2.0)
            .win(1, 2.0);

        let hand = parse_single_hand(&b).unwrap();
        let profit = net_profit(&hand, 1);
        // invested 3.0 (max bet on preflop), won 2.0, returned 2.0 → net = +1.0
        assert!((profit - 1.0).abs() < 0.001);
    }

    #[test]
    fn net_profit_with_antes() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .ante(1, 0.25)
            .ante(2, 0.25)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 2.0);

        let hand = parse_single_hand(&b).unwrap();
        let profit_p1 = net_profit(&hand, 1);
        // invested: ante 0.25 + sb 0.5 = 0.75, won 0, returned 0 → -0.75
        assert!((profit_p1 - (-0.75)).abs() < 0.001);
    }

    #[test]
    fn net_profit_with_dead_blind() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .dead_blind(1, 1.0) // extra posting, additive
            .fold(1)
            .fold(2)
            .win(3, 2.5);

        let hand = parse_single_hand(&b).unwrap();
        let profit_p1 = net_profit(&hand, 1);
        // invested: dead blind 1.0 (additive), won 0, returned 0 → -1.0
        assert!((profit_p1 - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn net_profit_dead_blind_plus_blind() {
        // Player posts both dead blind and small blind — both count.
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(3)
            .dead_blind(1, 1.0)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .fold(3)
            .win(2, 2.5);

        let hand = parse_single_hand(&b).unwrap();
        let profit_p1 = net_profit(&hand, 1);
        // invested: dead blind 1.0 (additive) + sb 0.5 (max on street) = 1.5
        assert!(
            (profit_p1 - (-1.5)).abs() < 0.001,
            "dead blind + sb should both count; got {profit_p1}"
        );
    }

    #[test]
    fn is_monetary_classification() {
        assert!(is_monetary(ActionType::SmallBlind));
        assert!(is_monetary(ActionType::BigBlind));
        assert!(is_monetary(ActionType::Ante));
        assert!(is_monetary(ActionType::Straddle));
        assert!(is_monetary(ActionType::DeadBlind));
        assert!(is_monetary(ActionType::Call));
        assert!(is_monetary(ActionType::Bet));
        assert!(!is_monetary(ActionType::Fold));
        assert!(!is_monetary(ActionType::Check));
    }

    #[test]
    fn real_showdown_requires_two_shows() {
        let b_one_show = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kd", "Qs"])
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["Ts", "9s"])
            .win(1, 2.0);

        let hand = parse_single_hand(&b_one_show).unwrap();
        assert!(!hand.real_showdown);

        let b_two_shows = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kd", "Qs"])
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["Ts", "9s"])
            .show(2, &["8s", "7s"])
            .win(1, 2.0);

        let hand = parse_single_hand(&b_two_shows).unwrap();
        assert!(hand.real_showdown);
    }

    #[test]
    fn shown_cards_populated() {
        let b = HandBuilder::new()
            .player_with_hand("p1", 1, "Alice", 100.0, &["As", "Kd"])
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .show(2, &["Qh", "Js"])
            .win(2, 1.5);

        let hand = parse_single_hand(&b).unwrap();
        assert!(hand.shown_cards.contains_key(&1));
        assert!(hand.shown_cards.contains_key(&2));
        let p1_cards = &hand.shown_cards[&1];
        assert_eq!(p1_cards.len(), 2);
    }

    #[test]
    fn hole_cards_from_show_event() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .showdown()
            .show(1, &["As", "Kd"])
            .show(2, &["Qh", "Js"])
            .win(1, 2.0);

        let hand = parse_single_hand(&b).unwrap();
        let p1 = hand.players.iter().find(|p| p.seat == 1).unwrap();
        assert!(p1.hole_cards.is_some());
        assert_eq!(p1.hole_cards.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn streets_parsed_correctly() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kd", "Qs"])
            .check(1)
            .check(2)
            .turn("Js")
            .check(1)
            .check(2)
            .river("Ts")
            .check(1)
            .check(2)
            .win(1, 2.0);

        let hand = parse_single_hand(&b).unwrap();
        assert_eq!(hand.streets.len(), 4);
        assert_eq!(hand.streets[0].street, Street::Preflop);
        assert_eq!(hand.streets[1].street, Street::Flop);
        assert_eq!(hand.streets[1].new_cards.len(), 3);
        assert_eq!(hand.streets[2].street, Street::Turn);
        assert_eq!(hand.streets[2].new_cards.len(), 1);
        assert_eq!(hand.streets[3].street, Street::River);
        assert_eq!(hand.streets[3].new_cards.len(), 1);
    }

    #[test]
    fn single_player_hand() {
        let b = HandBuilder::new().player("p1", 1, "Alice", 100.0).dealer(1).win(1, 0.0);

        let hand = parse_single_hand(&b).unwrap();
        assert_eq!(hand.players.len(), 1);
        assert_eq!(hand.players[0].position, Position::BTN);
    }

    #[test]
    fn all_in_flag_parsed() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet_all_in(1, 100.0)
            .call_all_in(2, 100.0)
            .win(1, 200.0);

        let hand = parse_single_hand(&b).unwrap();
        let preflop = &hand.streets[0];
        let allin_actions: Vec<_> = preflop.actions.iter().filter(|a| a.all_in).collect();
        assert_eq!(allin_actions.len(), 2);
    }

    #[test]
    fn player_names_tracked() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let data = parse_game_data(&b);
        assert!(data.player_names.contains_key("p1"));
        assert!(data.player_names.contains_key("p2"));
    }

    #[test]
    fn dead_blind_parsed() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .dead_blind(1, 1.0)
            .fold(1)
            .fold(2)
            .win(3, 2.5);

        let hand = parse_single_hand(&b).unwrap();
        let db_action =
            hand.streets[0].actions.iter().find(|a| a.kind == ActionType::DeadBlind).unwrap();
        assert_eq!(db_action.seat, 1);
        assert!((db_action.amount - 1.0).abs() < 0.001);
    }

    // Same name, different IDs across sessions → auto-unified without any config.
    #[test]
    fn auto_unify_same_name_multiple_ids() {
        let hand1 = HandBuilder::new()
            .player("id_alice_v1", 1, "Alice", 100.0)
            .player("id_bob", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let hand2 = HandBuilder::new()
            .player("id_alice_v2", 1, "Alice", 100.0)
            .player("id_bob", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        // No unify config — auto-unification by name
        let data = parse_multi_game_data(&[&hand1, &hand2]);

        let player_ids: std::collections::HashSet<_> =
            data.hands.iter().flat_map(|h| h.players.iter().map(|p| p.id.as_str())).collect();
        assert!(!player_ids.contains("id_alice_v2"), "id_alice_v2 should be unified away");
        assert!(player_ids.contains("id_alice_v1"), "id_alice_v1 should be the primary ID");
    }

    // Canonical config key that doesn't appear in game data: the first alias's ID becomes primary.
    #[test]
    fn unify_canonical_name_not_in_data() {
        let hand1 = HandBuilder::new()
            .player("id_andrew", 1, "Andrew", 100.0)
            .player("id_bob", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let hand2 = HandBuilder::new()
            .player("id_aryan", 1, "aryan", 100.0)
            .player("id_bob", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        // Config: andrew = ["Andrew", "aryan"] — "andrew" (lowercase) never appears in data
        let unify: HashMap<String, String> = [
            ("Andrew".to_string(), "andrew".to_string()),
            ("aryan".to_string(), "andrew".to_string()),
        ]
        .into_iter()
        .collect();
        let data = parse_multi_game_data_with_unify(&[&hand1, &hand2], &unify);

        let player_ids: std::collections::HashSet<_> =
            data.hands.iter().flat_map(|h| h.players.iter().map(|p| p.id.as_str())).collect();
        // Both should share the same ID (whichever was first)
        assert!(
            !player_ids.contains("id_andrew") || !player_ids.contains("id_aryan"),
            "Andrew and aryan should be unified into one ID"
        );
        assert_eq!(
            player_ids.iter().filter(|&&id| id != "id_bob").count(),
            1,
            "should be exactly one non-Bob player ID after unification"
        );
    }

    #[test]
    fn blind_remap_applied() {
        let b = HandBuilder::new()
            .blinds(1.0, 1.0)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 1.0)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 2.0);

        let remap = vec![crate::config::BlindRemap {
            from: [1.0, 1.0],
            to: [1.0, 2.0],
        }];
        let data = parse_game_data_with_remap(&b, &remap);
        let hand = &data.hands[0];
        assert_eq!(hand.small_blind, 1.0);
        assert_eq!(hand.big_blind, 2.0);
    }

    #[test]
    fn blind_remap_no_match_unchanged() {
        let b = HandBuilder::new()
            .blinds(0.5, 1.0)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let remap = vec![crate::config::BlindRemap {
            from: [1.0, 1.0],
            to: [1.0, 2.0],
        }];
        let data = parse_game_data_with_remap(&b, &remap);
        let hand = &data.hands[0];
        assert_eq!(hand.small_blind, 0.5);
        assert_eq!(hand.big_blind, 1.0);
    }

    #[test]
    fn blind_remap_affects_bb_normalized_stats() {
        let b = HandBuilder::new()
            .blinds(1.0, 1.0)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 1.0)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .win(1, 2.0);

        let no_remap = parse_game_data(&b);
        let stats_no = crate::stats::compute_stats(&no_remap);
        let s1_no = stats_no.iter().find(|s| s.player_id == "p1").unwrap();

        let remap = vec![crate::config::BlindRemap {
            from: [1.0, 1.0],
            to: [1.0, 2.0],
        }];
        let with_remap = parse_game_data_with_remap(&b, &remap);
        let stats_yes = crate::stats::compute_stats(&with_remap);
        let s1_yes = stats_yes.iter().find(|s| s.player_id == "p1").unwrap();

        // net_bb should be halved: same chip profit, double the BB
        assert!(
            (s1_no.net_bb - s1_yes.net_bb * 2.0).abs() < 0.001,
            "no_remap={}, with_remap={}",
            s1_no.net_bb,
            s1_yes.net_bb,
        );
    }
}
