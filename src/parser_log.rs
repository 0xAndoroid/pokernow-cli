use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::card::Card;
use crate::config::BlindRemap;
use crate::parser::{
    Action, ActionType, GameData, Hand, PlayerInHand, Position, Street, StreetData, Winner,
};

struct RawLogHand {
    header: String,
    player_lines: Vec<String>,
    action_lines: Vec<String>,
}

pub fn parse_log_files<S: std::hash::BuildHasher>(
    paths: &[String],
    unify: &HashMap<String, String, S>,
    blind_remap: &[BlindRemap],
) -> Result<GameData, Box<dyn std::error::Error>> {
    let mut all_hands = Vec::new();
    let mut player_names: HashMap<String, String> = HashMap::new();

    for path in paths {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let raw_hands = split_hands(reader);

        for raw in raw_hands {
            if let Some(hand) = process_log_hand(&raw, &mut player_names, unify, blind_remap) {
                all_hands.push(hand);
            }
        }
    }

    // Number hands sequentially
    for (i, hand) in all_hands.iter_mut().enumerate() {
        hand.number = (i + 1) as u32;
    }

    Ok(GameData {
        hands: all_hands,
        player_names,
    })
}

fn split_hands(reader: BufReader<File>) -> Vec<RawLogHand> {
    let mut hands = Vec::new();
    let mut current: Option<RawLogHand> = None;
    let mut in_players = false;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let trimmed = line.trim();

        if is_hand_header(trimmed) {
            if let Some(h) = current.take() {
                hands.push(h);
            }
            current = Some(RawLogHand {
                header: trimmed.to_string(),
                player_lines: Vec::new(),
                action_lines: Vec::new(),
            });
            in_players = true;
            continue;
        }

        let Some(ref mut hand) = current else { continue };

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.ends_with("players are in the hand") || trimmed.starts_with("you were dealt ") {
            in_players = false;
            continue;
        }

        if in_players {
            if trimmed.contains("{id: ") {
                if trimmed.contains("(bomb pot bet)") {
                    hand.action_lines.push(trimmed.to_string());
                } else {
                    hand.player_lines.push(trimmed.to_string());
                }
            }
            continue;
        }

        hand.action_lines.push(trimmed.to_string());
    }

    if let Some(h) = current {
        hands.push(h);
    }

    hands
}

fn is_hand_header(line: &str) -> bool {
    line.starts_with("No Limit Texas Hold'em - ")
        || line.starts_with("Pot Limit Omaha Hi - ")
        || line.starts_with("dealer: ")
        || line.starts_with("dead button - ")
}

fn is_holdem_header(header: &str) -> bool {
    header.starts_with("No Limit Texas Hold'em - ")
        || header.starts_with("dealer: ")
        || header.starts_with("dead button - ")
}

fn header_id(header: &str) -> String {
    // Extract date portion as ID
    if let Some(rest) = header.strip_prefix("No Limit Texas Hold'em - ") {
        return rest.replace(['/', ' ', ':'], "");
    }
    if let Some(rest) = header.strip_prefix("dead button - ") {
        return format!("db_{}", rest.replace(['/', ' ', ':'], ""));
    }
    if header.starts_with("dealer: ") {
        // dealer: "name @ id" - 2026/02/03 22:46:36 EST
        if let Some(idx) = header.find(" - ") {
            let date_part = &header[idx + 3..];
            return format!("dl_{}", date_part.replace(['/', ' ', ':'], ""));
        }
    }
    header.replace(['/', ' ', ':'], "")
}

struct ParsedPlayer {
    name: String,
    id: String,
    stack: f64,
    position: Position,
}

fn parse_player_line(line: &str) -> Option<ParsedPlayer> {
    // Format: name {id: xxx} (stack, POSITION)
    let id_start = line.find("{id: ")?;
    let name = line[..id_start].trim_end().to_string();

    let id_content_start = id_start + 5;
    let id_end = line[id_content_start..].find('}')? + id_content_start;
    let id = line[id_content_start..id_end].to_string();

    let paren_start = line[id_end..].find('(')? + id_end + 1;
    let paren_end = line[paren_start..].find(')')? + paren_start;
    let inner = &line[paren_start..paren_end];

    let comma = inner.rfind(',')?;
    let stack: f64 = inner[..comma].trim().parse().ok()?;
    let pos_str = inner[comma + 1..].trim();

    let position = match pos_str {
        "SB" => Position::SB,
        "BB" => Position::BB,
        "BU" => Position::BTN,
        "CO" => Position::CO,
        "MP" | "MP1" | "MP2" | "MP3" => Position::MP,
        _ => Position::EP,
    };

    Some(ParsedPlayer {
        name,
        id,
        stack,
        position,
    })
}

fn is_bomb_pot(action_lines: &[String]) -> bool {
    action_lines.iter().any(|l| l.contains("(bomb pot bet)"))
}

fn apply_blind_remap(sb: f64, bb: f64, rules: &[BlindRemap]) -> (f64, f64) {
    for rule in rules {
        if (rule.from[0] - sb).abs() < f64::EPSILON && (rule.from[1] - bb).abs() < f64::EPSILON {
            return (rule.to[0], rule.to[1]);
        }
    }
    (sb, bb)
}

fn process_log_hand<S: std::hash::BuildHasher>(
    raw: &RawLogHand,
    player_names: &mut HashMap<String, String>,
    unify: &HashMap<String, String, S>,
    blind_remap: &[BlindRemap],
) -> Option<Hand> {
    if !is_holdem_header(&raw.header) {
        return None;
    }
    if is_bomb_pot(&raw.action_lines) {
        return None;
    }

    let parsed_players: Vec<ParsedPlayer> =
        raw.player_lines.iter().filter_map(|l| parse_player_line(l)).collect();
    if parsed_players.is_empty() {
        return None;
    }

    let id = header_id(&raw.header);

    let mut players: Vec<PlayerInHand> = Vec::with_capacity(parsed_players.len());

    for (i, pp) in parsed_players.iter().enumerate() {
        let seat = (i + 1) as u8;
        let canonical_name = unify.get(&pp.name).map_or(&pp.name, |c| c);
        let resolved_id = player_names
            .iter()
            .find(|(_, n)| n.as_str() == canonical_name.as_str())
            .map_or_else(|| pp.id.clone(), |(id, _)| id.clone());
        player_names.entry(resolved_id.clone()).or_insert_with(|| canonical_name.clone());

        players.push(PlayerInHand {
            id: resolved_id,
            seat,
            name: canonical_name.clone(),
            stack: pp.stack,
            hole_cards: None,
            position: pp.position,
        });
    }

    // Build name→seat lookup (case-insensitive)
    let name_to_seat: HashMap<String, u8> =
        players.iter().map(|p| (p.name.to_ascii_lowercase(), p.seat)).collect();
    // Also map original (pre-unify) names
    let mut orig_name_to_seat: HashMap<String, u8> = name_to_seat.clone();
    for (i, pp) in parsed_players.iter().enumerate() {
        orig_name_to_seat.entry(pp.name.to_ascii_lowercase()).or_insert((i + 1) as u8);
    }

    // Determine blinds from posted amounts + positions
    let mut sb_amount = 0.0_f64;
    let mut bb_amount = 0.0_f64;

    for line in &raw.action_lines {
        if let Some((name, amount)) = parse_posted(line) {
            let seat = find_seat_by_name(&name, &orig_name_to_seat);
            if let Some(s) = seat {
                let pos = players.iter().find(|p| p.seat == s).map(|p| p.position);
                match pos {
                    Some(Position::SB) => sb_amount = amount,
                    Some(Position::BB) => bb_amount = amount,
                    _ => {
                        // Non-blind post (dead blind from non-SB/BB player)
                        // Use it to infer BB if we haven't found one
                        if bb_amount == 0.0 {
                            bb_amount = amount;
                        }
                    }
                }
            }
        }
    }

    // Fallback: if only one blind found (heads-up: SB posts first, BB second)
    if sb_amount == 0.0 && bb_amount > 0.0 {
        sb_amount = bb_amount;
    } else if bb_amount == 0.0 && sb_amount > 0.0 {
        bb_amount = sb_amount;
    }

    let (small_blind, big_blind) = apply_blind_remap(sb_amount, bb_amount, blind_remap);

    // Process actions
    let mut streets = vec![StreetData {
        street: Street::Preflop,
        new_cards: Vec::new(),
        actions: Vec::new(),
    }];
    let mut winners: Vec<Winner> = Vec::new();
    let mut shown_cards: HashMap<u8, Vec<Card>> = HashMap::new();
    let mut prev_board: Vec<Card> = Vec::new();

    for line in &raw.action_lines {
        if let Some(board_cards) = parse_board(line) {
            let new_cards = extract_new_cards(&prev_board, &board_cards);
            let street = match board_cards.len() {
                3 => Street::Flop,
                4 => Street::Turn,
                5 => Street::River,
                _ => continue,
            };
            streets.push(StreetData {
                street,
                new_cards,
                actions: Vec::new(),
            });
            prev_board = board_cards;
            continue;
        }

        if let Some((name, amount)) = parse_won(line) {
            let seat = find_seat_by_name(&name, &orig_name_to_seat).unwrap_or(0);
            winners.push(Winner {
                seat,
                amount,
                cards: shown_cards.get(&seat).cloned(),
                hand_description: None,
                run: 1,
            });
            continue;
        }

        if let Some((name, cards)) = parse_showed(line) {
            let seat = find_seat_by_name(&name, &orig_name_to_seat).unwrap_or(0);
            if seat > 0 {
                shown_cards.insert(seat, cards);
            }
            continue;
        }

        if let Some(action) = parse_action(line, &orig_name_to_seat, &players, bb_amount)
            && let Some(current) = streets.last_mut()
        {
            current.actions.push(action);
        }
    }

    // Compute uncalled returns from action sequence
    let uncalled_returns = compute_uncalled_returns(&streets, &winners);

    // Assign hole cards from shown_cards
    for p in &mut players {
        if let Some(cards) = shown_cards.get(&p.seat) {
            p.hole_cards = Some(cards.clone());
        }
    }

    let show_count = shown_cards.len();
    let real_showdown = show_count >= 2;

    // Detect run-it-twice: multiple wins for same hand
    // (log format doesn't have explicit RIT markers, but duplicate wins indicate it)
    let total_wins: usize = winners.len();
    let unique_won_seats: usize = {
        let mut seats: Vec<u8> = winners.iter().map(|w| w.seat).collect();
        seats.sort_unstable();
        seats.dedup();
        seats.len()
    };
    // RIT: multiple wins AND board went to river (otherwise it's side pots)
    // Actually we can't reliably detect RIT from text alone, so leave as false
    let run_it_twice = false;
    let _ = (total_wins, unique_won_seats); // suppress unused warnings

    let straddle =
        streets.iter().flat_map(|sd| sd.actions.iter()).find(|a| a.kind == ActionType::Straddle);
    let straddle_seat = straddle.map(|a| a.seat);
    let effective_bb = straddle.map_or(big_blind, |a| a.amount);

    // HU straddle anomaly: inferred SB > BB when the SB straddles
    let small_blind =
        if straddle_seat.is_some() && small_blind > big_blind { big_blind } else { small_blind };

    Some(Hand {
        id,
        number: 0, // filled in later
        small_blind,
        big_blind,
        effective_bb,
        straddle_seat,
        bomb_pot: false,
        players,
        streets,
        winners,
        real_showdown,
        shown_cards,
        uncalled_returns,
        run_it_twice,
        run2_cards: Vec::new(),
    })
}

fn parse_posted(line: &str) -> Option<(String, f64)> {
    let rest = line.strip_suffix(" and go all in").unwrap_or(line);
    let idx = rest.rfind(" posted ")?;
    let name = rest[..idx].to_string();
    let amount: f64 = rest[idx + 8..].parse().ok()?;
    Some((name, amount))
}

fn parse_won(line: &str) -> Option<(String, f64)> {
    let suffix = " chips";
    if !line.ends_with(suffix) {
        return None;
    }
    let rest = &line[..line.len() - suffix.len()];
    let idx = rest.rfind(" won ")?;
    let name = rest[..idx].to_string();
    let amount: f64 = rest[idx + 5..].parse().ok()?;
    Some((name, amount))
}

fn parse_showed(line: &str) -> Option<(String, Vec<Card>)> {
    let idx = line.find(" showed ")?;
    let name = line[..idx].to_string();
    let cards_str = &line[idx + 8..];
    let cards = parse_log_cards(cards_str);
    if cards.len() >= 2 { Some((name, cards)) } else { None }
}

fn parse_board(line: &str) -> Option<Vec<Card>> {
    let rest = line.strip_prefix("board: ")?;
    let cards = parse_log_cards(rest);
    if cards.len() >= 3 { Some(cards) } else { None }
}

fn parse_log_card(s: &str) -> Option<Card> {
    let s = s.trim().trim_end_matches(',');
    if s.is_empty() {
        return None;
    }
    let mut chars = s.chars();
    let rank_ch = chars.next()?;

    // Handle "10" as rank
    let (rank, suit_ch) = if rank_ch == '1' {
        let zero = chars.next()?;
        if zero != '0' {
            return None;
        }
        let suit = chars.next()?;
        (10_u8, suit)
    } else {
        let suit = chars.next()?;
        let rank = match rank_ch {
            '2'..='9' => rank_ch as u8 - b'0',
            'T' | 't' => 10,
            'J' | 'j' => 11,
            'Q' | 'q' => 12,
            'K' | 'k' => 13,
            'A' | 'a' => 14,
            _ => return None,
        };
        (rank, suit)
    };

    let suit = match suit_ch {
        '♣' | 'c' | 'C' => 0,
        '♦' | 'd' | 'D' => 1,
        '♥' | 'h' | 'H' => 2,
        '♠' | 's' | 'S' => 3,
        _ => return None,
    };

    if chars.next().is_some() {
        return None;
    }

    Some(Card::new(rank, suit))
}

fn parse_log_cards(s: &str) -> Vec<Card> {
    // Cards separated by spaces (first two in showed) and commas (rest in omaha showed)
    // "5♦ K♣ 8♣"  or  "10♦ A♦, 8♠, Q♥"
    // Split on both spaces and commas
    s.split([' ', ','])
        .filter(|t| !t.trim().is_empty())
        .filter_map(|t| parse_log_card(t.trim()))
        .collect()
}

fn extract_new_cards(prev_board: &[Card], full_board: &[Card]) -> Vec<Card> {
    if full_board.len() > prev_board.len() {
        full_board[prev_board.len()..].to_vec()
    } else {
        full_board.to_vec()
    }
}

fn find_seat_by_name(name: &str, name_to_seat: &HashMap<String, u8>) -> Option<u8> {
    name_to_seat.get(&name.to_ascii_lowercase()).copied()
}

fn parse_action(
    line: &str,
    name_to_seat: &HashMap<String, u8>,
    players: &[PlayerInHand],
    bb_amount: f64,
) -> Option<Action> {
    let all_in = line.ends_with(" and go all in");
    let line = if all_in { &line[..line.len() - " and go all in".len()] } else { line };

    // Try each action pattern
    if let Some((name, amount)) = parse_posted_action(line) {
        let seat = find_seat_by_name(&name, name_to_seat)?;
        let pos = players.iter().find(|p| p.seat == seat).map(|p| p.position)?;
        let kind = match pos {
            Position::SB => ActionType::SmallBlind,
            Position::BB => ActionType::BigBlind,
            _ if bb_amount > 0.0 && amount > bb_amount => ActionType::Straddle,
            _ => ActionType::DeadBlind,
        };
        return Some(Action {
            seat,
            kind,
            amount,
            all_in,
        });
    }

    if let Some(name) = parse_folded(line) {
        let seat = find_seat_by_name(&name, name_to_seat)?;
        return Some(Action {
            seat,
            kind: ActionType::Fold,
            amount: 0.0,
            all_in: false,
        });
    }

    if let Some(name) = parse_checked(line) {
        let seat = find_seat_by_name(&name, name_to_seat)?;
        return Some(Action {
            seat,
            kind: ActionType::Check,
            amount: 0.0,
            all_in: false,
        });
    }

    if let Some((name, amount)) = parse_called(line) {
        let seat = find_seat_by_name(&name, name_to_seat)?;
        return Some(Action {
            seat,
            kind: ActionType::Call,
            amount,
            all_in,
        });
    }

    if let Some((name, amount)) = parse_raised_to(line) {
        let seat = find_seat_by_name(&name, name_to_seat)?;
        return Some(Action {
            seat,
            kind: ActionType::Bet,
            amount,
            all_in,
        });
    }

    if let Some((name, amount)) = parse_bet(line) {
        let seat = find_seat_by_name(&name, name_to_seat)?;
        return Some(Action {
            seat,
            kind: ActionType::Bet,
            amount,
            all_in,
        });
    }

    None
}

fn parse_posted_action(line: &str) -> Option<(String, f64)> {
    let idx = line.rfind(" posted ")?;
    let name = line[..idx].to_string();
    let amount: f64 = line[idx + 8..].parse().ok()?;
    Some((name, amount))
}

fn parse_folded(line: &str) -> Option<String> {
    line.strip_suffix(" folded").map(String::from)
}

fn parse_checked(line: &str) -> Option<String> {
    line.strip_suffix(" checked").map(String::from)
}

fn parse_called(line: &str) -> Option<(String, f64)> {
    let idx = line.rfind(" called ")?;
    let name = line[..idx].to_string();
    let amount: f64 = line[idx + 8..].parse().ok()?;
    Some((name, amount))
}

fn parse_raised_to(line: &str) -> Option<(String, f64)> {
    let idx = line.rfind(" raised to ")?;
    let name = line[..idx].to_string();
    let amount: f64 = line[idx + 11..].parse().ok()?;
    Some((name, amount))
}

fn parse_bet(line: &str) -> Option<(String, f64)> {
    let idx = line.rfind(" bet ")?;
    let name = line[..idx].to_string();
    let amount: f64 = line[idx + 5..].parse().ok()?;
    Some((name, amount))
}

fn compute_uncalled_returns(streets: &[StreetData], winners: &[Winner]) -> HashMap<u8, f64> {
    use crate::parser::is_monetary;

    let mut returns: HashMap<u8, f64> = HashMap::new();

    // Total invested per seat across all streets
    let mut total_invested: HashMap<u8, f64> = HashMap::new();
    for sd in streets {
        let mut street_max: HashMap<u8, f64> = HashMap::new();
        let mut street_additive: HashMap<u8, f64> = HashMap::new();
        for a in &sd.actions {
            if !is_monetary(a.kind) {
                continue;
            }
            if matches!(a.kind, ActionType::Ante | ActionType::DeadBlind) {
                *street_additive.entry(a.seat).or_insert(0.0) += a.amount;
            } else {
                let entry = street_max.entry(a.seat).or_insert(0.0);
                *entry = entry.max(a.amount);
            }
        }
        for (seat, max) in &street_max {
            *total_invested.entry(*seat).or_insert(0.0) += max;
        }
        for (seat, add) in &street_additive {
            *total_invested.entry(*seat).or_insert(0.0) += add;
        }
    }

    let total_won: f64 = winners.iter().map(|w| w.amount).sum();
    let total_put_in: f64 = total_invested.values().sum();

    // If total won < total invested, the difference went back to someone
    if total_put_in > total_won + 0.001 {
        let uncalled = total_put_in - total_won;
        // Find the seat that invested the most (the uncalled bettor)
        if let Some((&seat, _)) = total_invested.iter().max_by(|(_, a), (_, b)| a.total_cmp(b)) {
            returns.insert(seat, uncalled);
        }
    }

    returns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_log_card_unicode() {
        let c = parse_log_card("5♦").unwrap();
        assert_eq!(c, Card::new(5, 1)); // 5 of diamonds
        let c = parse_log_card("K♣").unwrap();
        assert_eq!(c, Card::new(13, 0)); // K of clubs
        let c = parse_log_card("A♠").unwrap();
        assert_eq!(c, Card::new(14, 3)); // A of spades
        let c = parse_log_card("Q♥").unwrap();
        assert_eq!(c, Card::new(12, 2)); // Q of hearts
    }

    #[test]
    fn parse_log_card_10() {
        let c = parse_log_card("10♦").unwrap();
        assert_eq!(c, Card::new(10, 1));
        let c = parse_log_card("10♣").unwrap();
        assert_eq!(c, Card::new(10, 0));
    }

    #[test]
    fn parse_log_card_invalid() {
        assert!(parse_log_card("").is_none());
        assert!(parse_log_card("X♦").is_none());
        assert!(parse_log_card("5Z").is_none());
    }

    #[test]
    fn parse_log_cards_board() {
        let cards = parse_log_cards("5♦ K♣ 8♣");
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0], Card::new(5, 1));
        assert_eq!(cards[1], Card::new(13, 0));
        assert_eq!(cards[2], Card::new(8, 0));
    }

    #[test]
    fn parse_log_cards_with_10() {
        let cards = parse_log_cards("10♦ A♦ 8♠");
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0], Card::new(10, 1));
    }

    #[test]
    fn parse_log_cards_omaha_show() {
        // Omaha 4-card: first two space-sep, rest comma-sep
        let cards = parse_log_cards("10♦ A♦, 8♠, Q♥");
        assert_eq!(cards.len(), 4);
    }

    #[test]
    fn parse_board_line() {
        let cards = parse_board("board: 5♦ K♣ 8♣").unwrap();
        assert_eq!(cards.len(), 3);
    }

    #[test]
    fn parse_board_not_board() {
        assert!(parse_board("kevin bet 5").is_none());
    }

    #[test]
    fn parse_won_line() {
        let (name, amount) = parse_won("kevin won 31 chips").unwrap();
        assert_eq!(name, "kevin");
        assert!((amount - 31.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_won_decimal() {
        let (name, amount) = parse_won("Steve won 5.00 chips").unwrap();
        assert_eq!(name, "Steve");
        assert!((amount - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_won_zero() {
        let (name, amount) = parse_won("alex won 0 chips").unwrap();
        assert_eq!(name, "alex");
        assert!((amount - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_showed_line() {
        let (name, cards) = parse_showed("nate showed 5♦ K♦").unwrap();
        assert_eq!(name, "nate");
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0], Card::new(5, 1));
        assert_eq!(cards[1], Card::new(13, 1));
    }

    #[test]
    fn parse_player_line_basic() {
        let p = parse_player_line("om {id: viPKe3plzj} (703, SB)").unwrap();
        assert_eq!(p.name, "om");
        assert_eq!(p.id, "viPKe3plzj");
        assert!((p.stack - 703.0).abs() < f64::EPSILON);
        assert_eq!(p.position, Position::SB);
    }

    #[test]
    fn parse_player_line_decimal_stack() {
        let p = parse_player_line("rc {id: oDssX0s202} (443.68, SB)").unwrap();
        assert_eq!(p.name, "rc");
        assert!((p.stack - 443.68).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_player_line_positions() {
        let check = |line: &str, expected: Position| {
            let p = parse_player_line(line).unwrap();
            assert_eq!(p.position, expected);
        };
        check("a {id: x} (100, BU)", Position::BTN);
        check("a {id: x} (100, CO)", Position::CO);
        check("a {id: x} (100, UTG)", Position::EP);
        check("a {id: x} (100, MP)", Position::MP);
        check("a {id: x} (100, MP1)", Position::MP);
        check("a {id: x} (100, MP2)", Position::MP);
        check("a {id: x} (100, MP3)", Position::MP);
    }

    #[test]
    fn parse_posted_line() {
        let (name, amount) = parse_posted("om posted 1").unwrap();
        assert_eq!(name, "om");
        assert!((amount - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_posted_decimal() {
        let (name, amount) = parse_posted("nate posted 0.50").unwrap();
        assert_eq!(name, "nate");
        assert!((amount - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_action_folded() {
        assert_eq!(parse_folded("nate folded").unwrap(), "nate");
    }

    #[test]
    fn parse_action_checked() {
        assert_eq!(parse_checked("Ro checked").unwrap(), "Ro");
    }

    #[test]
    fn parse_action_called() {
        let (name, amount) = parse_called("Andrew called 6").unwrap();
        assert_eq!(name, "Andrew");
        assert!((amount - 6.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_action_raised_to() {
        let (name, amount) = parse_raised_to("kevin raised to 6").unwrap();
        assert_eq!(name, "kevin");
        assert!((amount - 6.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_action_bet() {
        let (name, amount) = parse_bet("kevin bet 15").unwrap();
        assert_eq!(name, "kevin");
        assert!((amount - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn header_detection() {
        assert!(is_hand_header("No Limit Texas Hold'em - 2025/12/13 22:33:59 EST"));
        assert!(is_hand_header("Pot Limit Omaha Hi - 2025/12/14 18:57:08 EST"));
        assert!(is_hand_header("dealer: \"kev @ lWyn_wrdD6\" - 2026/02/03 22:46:36 EST"));
        assert!(is_hand_header("dead button - 2026/02/03 23:14:59 EST"));
        assert!(!is_hand_header("kevin bet 15"));
    }

    #[test]
    fn holdem_header_detection() {
        assert!(is_holdem_header("No Limit Texas Hold'em - 2025/12/13 22:33:59 EST"));
        assert!(is_holdem_header("dealer: \"kev @ lWyn_wrdD6\" - 2026/02/03 22:46:36 EST"));
        assert!(is_holdem_header("dead button - 2026/02/03 23:14:59 EST"));
        assert!(!is_holdem_header("Pot Limit Omaha Hi - 2025/12/14 18:57:08 EST"));
    }

    #[test]
    fn extract_new_cards_flop() {
        let prev: Vec<Card> = vec![];
        let full = vec![Card::new(5, 1), Card::new(13, 0), Card::new(8, 0)];
        let new = extract_new_cards(&prev, &full);
        assert_eq!(new.len(), 3);
    }

    #[test]
    fn extract_new_cards_turn() {
        let prev = vec![Card::new(5, 1), Card::new(13, 0), Card::new(8, 0)];
        let full = vec![Card::new(5, 1), Card::new(13, 0), Card::new(8, 0), Card::new(8, 1)];
        let new = extract_new_cards(&prev, &full);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0], Card::new(8, 1));
    }

    #[test]
    fn header_id_generation() {
        let id = header_id("No Limit Texas Hold'em - 2025/12/13 22:33:59 EST");
        assert!(!id.is_empty());
        assert!(!id.contains('/'));

        let id2 = header_id("dealer: \"kev @ lWyn_wrdD6\" - 2026/02/03 22:46:36 EST");
        assert!(id2.starts_with("dl_"));

        let id3 = header_id("dead button - 2026/02/03 23:14:59 EST");
        assert!(id3.starts_with("db_"));
    }

    #[test]
    fn full_hand_parse() {
        let input = "\
No Limit Texas Hold'em - 2025/12/13 22:33:59 EST
om {id: viPKe3plzj} (703, SB)
Andrew {id: wBcyK_YnY6} (173, BU)
kevin {id: lWyn_wrdD6} (64, BB)
3 players are in the hand
om posted 1
kevin posted 1
Andrew raised to 6
om folded
kevin called 6
board: 5♦ K♣ 8♣
kevin checked
Andrew bet 10
kevin folded
Andrew won 13 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        assert_eq!(data.hands.len(), 1);
        let hand = &data.hands[0];
        assert_eq!(hand.number, 1);
        assert_eq!(hand.players.len(), 3);
        assert!((hand.small_blind - 1.0).abs() < f64::EPSILON);
        assert!((hand.big_blind - 1.0).abs() < f64::EPSILON);
        assert_eq!(hand.winners.len(), 1);
        assert!((hand.winners[0].amount - 13.0).abs() < f64::EPSILON);
        // Preflop + flop = 2 streets
        assert_eq!(hand.streets.len(), 2);
        assert_eq!(hand.streets[1].street, Street::Flop);
        assert_eq!(hand.streets[1].new_cards.len(), 3);
    }

    #[test]
    fn skip_omaha_hands() {
        let input = "\
Pot Limit Omaha Hi - 2025/12/14 18:57:08 EST
rc {id: oDssX0s202} (443.68, SB)
Andrew {id: wBcyK_YnY6} (401.75, BB)
2 players are in the hand
rc posted 1
Andrew posted 1
rc folded
Andrew won 2 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();
        assert!(data.hands.is_empty());
    }

    #[test]
    fn skip_bomb_pots() {
        let input = "\
No Limit Texas Hold'em - 2026/01/04 18:13:19 EST
rc {id: oDssX0s202} (443.68, SB)
Andrew {id: wBcyK_YnY6} (401.75, BB)
Nick {id: K1wuJXY-OF} called 2.00 (bomb pot bet)
rc {id: oDssX0s202} called 2.00 (bomb pot bet)
Andrew {id: wBcyK_YnY6} called 2.00 (bomb pot bet)
9 players are in the hand
board: 7♠ 4♥ 7♦
Andrew won 6.00 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();
        assert!(data.hands.is_empty());
    }

    #[test]
    fn multiple_hands_sequential_numbering() {
        let input = "\
No Limit Texas Hold'em - 2025/12/13 22:33:59 EST
om {id: viPKe3plzj} (703, SB)
kevin {id: lWyn_wrdD6} (64, BB)
2 players are in the hand
om posted 1
kevin posted 1
om folded
kevin won 2 chips

No Limit Texas Hold'em - 2025/12/13 22:34:40 EST
om {id: viPKe3plzj} (702, SB)
kevin {id: lWyn_wrdD6} (65, BB)
2 players are in the hand
om posted 1
kevin posted 1
kevin folded
om won 2 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        assert_eq!(data.hands.len(), 2);
        assert_eq!(data.hands[0].number, 1);
        assert_eq!(data.hands[1].number, 2);
    }

    #[test]
    fn showdown_detection() {
        let input = "\
No Limit Texas Hold'em - 2025/12/13 22:36:59 EST
nate {id: 0pg_uRrG6e} (96, SB)
Ro {id: ZnwfUGcFrg} (140, BB)
2 players are in the hand
nate posted 1
Ro posted 1
nate called 1
Ro checked
board: 7♦ A♣ 7♣
nate checked
Ro checked
board: 7♦ A♣ 7♣ 4♣
nate checked
Ro checked
board: 7♦ A♣ 7♣ 4♣ 6♠
nate checked
Ro checked
nate showed 5♦ K♦
Ro showed 3♦ 6♥
Ro won 2 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        assert_eq!(data.hands.len(), 1);
        let hand = &data.hands[0];
        assert!(hand.real_showdown);
        assert_eq!(hand.shown_cards.len(), 2);
        assert_eq!(hand.streets.len(), 4); // preflop + flop + turn + river
    }

    #[test]
    fn all_in_action() {
        let input = "\
No Limit Texas Hold'em - 2025/12/13 22:36:59 EST
alex {id: HWXAyjhT5g} (23, SB)
kevin {id: lWyn_wrdD6} (86, BB)
2 players are in the hand
alex posted 1
kevin posted 1
alex raised to 23 and go all in
kevin called 23
alex showed 2♣ A♥
kevin showed A♦ 9♦
board: 7♣ A♣ 5♥ A♠ 8♥
kevin won 46 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        assert_eq!(data.hands.len(), 1);
        let hand = &data.hands[0];
        // Check all-in was parsed
        let preflop = &hand.streets[0];
        let raise_action = preflop.actions.iter().find(|a| a.kind == ActionType::Bet).unwrap();
        assert!(raise_action.all_in);
    }

    #[test]
    fn dealer_header_parsed() {
        let input = "\
dealer: \"kev @ lWyn_wrdD6\" - 2026/02/03 22:46:36 EST
kev {id: lWyn_wrdD6} (238, BB)
om {id: SEqtk4oefp} (362, SB)
2 players are in the hand

om posted 1
kev posted 1
om raised to 4
kev called 4
board: 5♣ 3♥ 3♦
om bet 5
kev called 5
board: 5♣ 3♥ 3♦ 7♣
om bet 12
kev called 12
board: 5♣ 3♥ 3♦ 7♣ Q♣
om checked
kev checked
om showed A♠ 3♣
om won 42 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        assert_eq!(data.hands.len(), 1);
        let hand = &data.hands[0];
        assert!(hand.id.starts_with("dl_"));
        assert_eq!(hand.players.len(), 2);
    }

    #[test]
    fn dead_button_header_parsed() {
        let input = "\
dead button - 2026/02/03 23:14:59 EST
kev {id: lWyn_wrdD6} (273, SB)
Andrew {id: wBcyK_YnY6} (362, BB)
2 players are in the hand
kev posted 1
Andrew posted 1
kev folded
Andrew won 2 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        assert_eq!(data.hands.len(), 1);
        assert!(data.hands[0].id.starts_with("db_"));
    }

    #[test]
    fn player_unification() {
        let input = "\
No Limit Texas Hold'em - 2025/12/13 22:33:59 EST
om {id: viPKe3plzj} (703, SB)
om__g {id: vrxe14spS8} (696, BB)
2 players are in the hand
om posted 1
om__g posted 1
om folded
om__g won 2 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let mut unify = HashMap::new();
        unify.insert("om__g".to_string(), "om".to_string());
        let data = parse_log_files(&[path], &unify, &[]).unwrap();

        assert_eq!(data.hands.len(), 1);
        // Both players should have "om" as their canonical name
        let names: Vec<&str> = data.hands[0].players.iter().map(|p| p.name.as_str()).collect();
        assert!(names.iter().all(|n| *n == "om"));
    }

    #[test]
    fn uncalled_return_computation() {
        let input = "\
No Limit Texas Hold'em - 2025/12/13 22:33:59 EST
om {id: viPKe3plzj} (703, SB)
kevin {id: lWyn_wrdD6} (64, BB)
2 players are in the hand
om posted 1
kevin posted 1
om raised to 10
kevin folded
om won 2 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        let hand = &data.hands[0];
        // om invested 10, kevin invested 1, total pot should be 11 but om won 2
        // So uncalled return = 10 - 1 = 9 but actual: won 2 = 1(SB blind that stayed) + 1(BB)
        // Actually om gets back 8 uncalled
        // total invested = 10 + 1 = 11, won = 2, uncalled = 11 - 2 = 9
        let om_seat = 1_u8;
        assert!(hand.uncalled_returns.contains_key(&om_seat));
    }

    #[test]
    fn straddle_detected_in_log() {
        let input = "\
No Limit Texas Hold'em - 2025/12/28 20:00:00 EST
om {id: viPKe3plzj} (100, SB)
kevin {id: lWyn_wrdD6} (100, BB)
Andrew {id: wBcyK_YnY6} (100, BU)
3 players are in the hand
om posted 0.5
kevin posted 1
Andrew posted 2
om folded
kevin folded
Andrew won 3.5 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        assert_eq!(data.hands.len(), 1);
        let hand = &data.hands[0];
        assert_eq!(hand.straddle_seat, Some(3)); // Andrew = seat 3 (third player)
        assert!((hand.effective_bb - 2.0).abs() < f64::EPSILON);
        assert!((hand.big_blind - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dead_blind_not_mistaken_for_straddle() {
        let input = "\
No Limit Texas Hold'em - 2025/12/28 20:00:00 EST
om {id: viPKe3plzj} (100, SB)
kevin {id: lWyn_wrdD6} (100, BB)
Andrew {id: wBcyK_YnY6} (100, BU)
3 players are in the hand
om posted 0.5
kevin posted 1
Andrew posted 1
om folded
kevin folded
Andrew won 2.5 chips
";
        let file = write_temp_log(input);
        let path = file.path().to_string_lossy().to_string();
        let data = parse_log_files(&[path], &HashMap::new(), &[]).unwrap();

        let hand = &data.hands[0];
        assert!(hand.straddle_seat.is_none(), "posting BB amount is dead blind, not straddle");
        assert!((hand.effective_bb - 1.0).abs() < f64::EPSILON);
    }

    fn write_temp_log(content: &str) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".log").unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }
}
