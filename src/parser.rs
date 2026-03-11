use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use serde::Deserialize;

use crate::card::Card;

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
    #[allow(dead_code)]
    bomb_pot: Option<bool>,
}

pub fn parse_files(
    paths: &[String],
    unify: &HashMap<String, String>,
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
                player_names.insert(canonical.clone(), rp.name.clone());
            }

            if let Some(hand) = process_hand(&raw_hand, &id_unify) {
                all_hands.push(hand);
            }
        }
    }

    Ok(GameData {
        hands: all_hands,
        player_names,
    })
}

fn build_id_unify_map(
    paths: &[String],
    unify: &HashMap<String, String>,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    if unify.is_empty() {
        return Ok(HashMap::new());
    }

    let mut name_to_id: HashMap<String, String> = HashMap::new();

    for path in paths {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let raw: RawGameFile = serde_json::from_reader(reader)?;
        for raw_hand in &raw.hands {
            for rp in &raw_hand.players {
                name_to_id.entry(rp.name.clone()).or_insert(rp.id.clone());
            }
        }
    }

    let mut id_map: HashMap<String, String> = HashMap::new();
    for (name, canonical_name) in unify {
        let canonical_id = name_to_id.get(canonical_name.as_str());
        let source_id = name_to_id.get(name.as_str());
        if let (Some(src), Some(canon)) = (source_id, canonical_id)
            && src != canon
        {
            id_map.insert(src.clone(), canon.clone());
        }
    }

    Ok(id_map)
}

fn process_hand(raw: &RawHand, id_unify: &HashMap<String, String>) -> Option<Hand> {
    match raw.game_type.as_str() {
        "th" | "omaha" => {}
        _ => return None,
    }

    let number: u32 = raw.number.parse().unwrap_or(0);
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

    let (mut streets, winners, uncalled_returns, show_count) =
        process_events(&raw.events, &mut shown_cards);

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
        small_blind: raw.small_blind,
        big_blind: raw.big_blind,
        bomb_pot: raw.bomb_pot,
        players,
        streets,
        winners,
        real_showdown,
        shown_cards,
        uncalled_returns,
    })
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

fn process_events(
    events: &[RawEvent],
    shown_cards: &mut HashMap<u8, Vec<Card>>,
) -> (Vec<StreetData>, Vec<Winner>, HashMap<u8, f64>, usize) {
    let mut streets = vec![StreetData {
        street: Street::Preflop,
        new_cards: Vec::new(),
        actions: Vec::new(),
    }];
    let mut winners = Vec::new();
    let mut uncalled_returns: HashMap<u8, f64> = HashMap::new();
    let mut show_count: usize = 0;

    for ev in events {
        let p = &ev.payload;

        if p.event_type == 9 {
            if p.run.unwrap_or(1) != 1 {
                continue;
            }
            let street = match p.turn.unwrap_or(0) {
                1 => Street::Flop,
                2 => Street::Turn,
                3 => Street::River,
                _ => continue,
            };
            let new_cards =
                p.cards.as_ref().map(|cs| parse_optional_card_vec(cs)).unwrap_or_default();
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
                winners.push(Winner {
                    seat,
                    amount: p.value.unwrap_or(0.0),
                    cards,
                    hand_description: p.hand_description.clone(),
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

    (streets, winners, uncalled_returns, show_count)
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

pub fn net_profit(hand: &Hand, seat: u8) -> f64 {
    let mut ante_total = 0.0_f64;
    let mut street_maxes = [0.0_f64; 4];
    for (i, sd) in hand.streets.iter().enumerate() {
        for a in &sd.actions {
            if a.seat == seat && is_monetary(a.kind) {
                if a.kind == ActionType::Ante {
                    ante_total += a.amount;
                } else {
                    street_maxes[i] = street_maxes[i].max(a.amount);
                }
            }
        }
    }
    let invested: f64 = ante_total + street_maxes.iter().sum::<f64>();
    let won: f64 = hand.winners.iter().filter(|w| w.seat == seat).map(|w| w.amount).sum();
    let returned = hand.uncalled_returns.get(&seat).copied().unwrap_or(0.0);
    won + returned - invested
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
