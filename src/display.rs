use std::collections::HashMap;

use crate::card::{self, Card};
use crate::parser::{ActionType, Hand, Position, Street, is_monetary};

const POSITION_ORDER: [Position; 6] =
    [Position::BTN, Position::SB, Position::BB, Position::EP, Position::MP, Position::CO];

pub fn display_hand(hand: &Hand) {
    let seat_name = build_seat_name_map(hand);
    let hole_cards = build_hole_cards_map(hand);
    let bb = hand.big_blind;

    print_header(hand, &seat_name);
    print_players(hand, &hole_cards, bb);

    let mut board: Vec<Card> = Vec::new();
    let mut running_pot = 0.0;

    for (street_idx, sd) in hand.streets.iter().enumerate() {
        let street_pot = compute_street_pot(hand, street_idx);
        print_street_header(sd.street, &sd.new_cards, &board, running_pot);

        if sd.street != Street::Preflop {
            board.extend_from_slice(&sd.new_cards);
        }

        if sd.street != Street::Preflop {
            let folded = collect_folded_seats(hand, street_idx);
            print_made_hands(&hole_cards, &board, &seat_name, &folded);
        }

        print_actions(&sd.actions, &seat_name, sd.street);
        running_pot += street_pot;
    }

    print_results(hand, &seat_name);
}

fn build_seat_name_map(hand: &Hand) -> HashMap<u8, String> {
    hand.players.iter().map(|p| (p.seat, p.name.clone())).collect()
}

fn build_hole_cards_map(hand: &Hand) -> HashMap<u8, Vec<Card>> {
    let mut map: HashMap<u8, Vec<Card>> = HashMap::new();
    for p in &hand.players {
        if let Some(ref cards) = p.hole_cards {
            map.insert(p.seat, cards.clone());
        }
    }
    for (&seat, cards) in &hand.shown_cards {
        map.entry(seat).or_insert_with(|| cards.clone());
    }
    map
}

fn print_header(hand: &Hand, seat_name: &HashMap<u8, String>) {
    let bomb = if hand.bomb_pot { " [BOMB POT]" } else { "" };
    println!(
        "Hand #{} | Stakes {}/{} | {} players{}",
        hand.number,
        format_chips(hand.small_blind),
        format_chips(hand.big_blind),
        hand.players.len(),
        bomb,
    );

    if let Some(dealer) = hand.players.iter().find(|p| p.position == Position::BTN) {
        let name = seat_name.get(&dealer.seat).map_or("?", String::as_str);
        println!("Dealer: {} (BTN)", name);
    }

    println!();
}

fn print_players(hand: &Hand, hole_cards: &HashMap<u8, Vec<Card>>, bb: f64) {
    println!("Players:");

    let mut sorted: Vec<_> = hand.players.iter().collect();
    sorted.sort_by_key(|p| {
        POSITION_ORDER.iter().position(|&pos| pos == p.position).unwrap_or(usize::MAX)
    });

    for p in sorted {
        let pos_str = format!("{:4}", position_tag(p.position));
        let stack_bb = p.stack / bb;
        let cards_str = match hole_cards.get(&p.seat) {
            Some(cards) => format!("  [{}]", format_cards(cards)),
            None => String::new(),
        };
        println!("  {}{:<8} {} BB{}", pos_str, p.name, format_bb(stack_bb), cards_str,);
    }

    println!();
}

fn compute_street_pot(hand: &Hand, street_idx: usize) -> f64 {
    let sd = &hand.streets[street_idx];
    let mut per_player: HashMap<u8, f64> = HashMap::new();
    for a in &sd.actions {
        if is_monetary(a.kind) {
            let entry = per_player.entry(a.seat).or_insert(0.0);
            if a.amount > *entry {
                *entry = a.amount;
            }
        }
    }
    per_player.values().sum()
}

fn print_street_header(street: Street, new_cards: &[Card], board: &[Card], pot: f64) {
    let header = match street {
        Street::Preflop => format!("=== PREFLOP === (pot: {})", format_chips(pot)),
        Street::Flop => {
            format!("=== FLOP [{}] === (pot: {})", format_cards(new_cards), format_chips(pot),)
        }
        Street::Turn => {
            format!(
                "=== TURN [{}] [{}] === (pot: {})",
                format_cards(board),
                format_cards(new_cards),
                format_chips(pot),
            )
        }
        Street::River => {
            format!(
                "=== RIVER [{}] [{}] === (pot: {})",
                format_cards(board),
                format_cards(new_cards),
                format_chips(pot),
            )
        }
    };
    println!("{header}");
}

fn collect_folded_seats(hand: &Hand, up_to_street: usize) -> Vec<u8> {
    let mut folded = Vec::new();
    for sd in &hand.streets[..up_to_street] {
        for a in &sd.actions {
            if a.kind == ActionType::Fold {
                folded.push(a.seat);
            }
        }
    }
    folded
}

fn print_made_hands(
    hole_cards: &HashMap<u8, Vec<Card>>,
    board: &[Card],
    seat_name: &HashMap<u8, String>,
    folded: &[u8],
) {
    let mut entries: Vec<(&str, String)> = Vec::new();
    for (&seat, cards) in hole_cards {
        if folded.contains(&seat) {
            continue;
        }
        let name = seat_name.get(&seat).map_or("?", String::as_str);
        let desc = card::holding_description(cards, board);
        entries.push((name, desc));
    }
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (name, desc) in entries {
        println!("  {name}: {desc}");
    }
}

fn print_actions(
    actions: &[crate::parser::Action],
    seat_name: &HashMap<u8, String>,
    street: Street,
) {
    let mut had_bet = street == Street::Preflop;

    for a in actions {
        let name = seat_name.get(&a.seat).map_or("?", String::as_str);
        let all_in_tag = if a.all_in { " (all-in)" } else { "" };

        let line = match a.kind {
            ActionType::SmallBlind => {
                format!("  {} posts small blind {}", name, format_chips(a.amount))
            }
            ActionType::BigBlind => {
                format!("  {} posts big blind {}", name, format_chips(a.amount))
            }
            ActionType::Ante => format!("  {} antes {}", name, format_chips(a.amount)),
            ActionType::Straddle => format!("  {} straddles {}", name, format_chips(a.amount)),
            ActionType::DeadBlind => {
                format!("  {} posts dead blind {}", name, format_chips(a.amount))
            }
            ActionType::Fold => format!("  {name} folds"),
            ActionType::Check => format!("  {name} checks"),
            ActionType::Call => {
                format!("  {} calls {}{}", name, format_chips(a.amount), all_in_tag,)
            }
            ActionType::Bet => {
                let verb = if had_bet { "raises to" } else { "bets" };
                had_bet = true;
                format!("  {} {} {}{}", name, verb, format_chips(a.amount), all_in_tag,)
            }
        };
        println!("{line}");
    }

    println!();
}

fn print_results(hand: &Hand, seat_name: &HashMap<u8, String>) {
    if hand.winners.is_empty() {
        return;
    }

    println!("Result:");
    for w in &hand.winners {
        let name = seat_name.get(&w.seat).map_or("?", String::as_str);
        match &w.hand_description {
            Some(desc) => println!("  {} wins {} ({})", name, format_chips(w.amount), desc),
            None => println!("  {} wins {}", name, format_chips(w.amount)),
        }
    }
}

fn format_cards(cards: &[Card]) -> String {
    cards.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
}

fn format_chips(amount: f64) -> String {
    if amount.fract() == 0.0 { format!("{}", amount as i64) } else { format!("{amount}") }
}

fn format_bb(amount: f64) -> String {
    if (amount - amount.round()).abs() < 0.01 {
        format!("{}", amount.round() as i64)
    } else {
        format!("{amount:.1}")
    }
}

fn position_tag(pos: Position) -> &'static str {
    match pos {
        Position::BTN => "BTN",
        Position::SB => "SB",
        Position::BB => "BB",
        Position::EP => "EP",
        Position::MP => "MP",
        Position::CO => "CO",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_helpers::*;

    #[test]
    fn display_hand_no_panic() {
        let b = HandBuilder::new()
            .player_with_hand("p1", 1, "Alice", 100.0, &["As", "Kd"])
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .call(2, 3.0)
            .fold(3)
            .flop(&["Ah", "Kh", "Qs"])
            .bet(1, 5.0)
            .call(2, 5.0)
            .turn("Js")
            .bet(1, 10.0)
            .fold(2)
            .win(1, 26.0);

        let hand = parse_single_hand(&b).unwrap();
        display_hand(&hand);
    }

    #[test]
    fn display_hand_through_river() {
        let b = HandBuilder::new()
            .player_with_hand("p1", 1, "Alice", 100.0, &["As", "Kd"])
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kh", "Qs"])
            .check(1)
            .check(2)
            .turn("Js")
            .check(1)
            .check(2)
            .river("Ts")
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["As", "Kd"])
            .show(2, &["9s", "8s"])
            .win(1, 2.0);

        let hand = parse_single_hand(&b).unwrap();
        display_hand(&hand);
    }

    #[test]
    fn display_hand_with_all_in() {
        let b = HandBuilder::new()
            .player_with_hand("p1", 1, "Alice", 100.0, &["As", "Kd"])
            .player("p2", 2, "Bob", 50.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet_all_in(1, 100.0)
            .call_all_in(2, 50.0)
            .uncalled_return(1, 50.0)
            .flop(&["Ah", "Kh", "Qs"])
            .turn("Js")
            .river("Ts")
            .showdown()
            .show(1, &["As", "Kd"])
            .show(2, &["9s", "8s"])
            .win(1, 100.0);

        let hand = parse_single_hand(&b).unwrap();
        display_hand(&hand);
    }

    #[test]
    fn display_preflop_only() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let hand = parse_single_hand(&b).unwrap();
        display_hand(&hand);
    }

    #[test]
    fn display_hand_no_winners() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1);

        let hand = parse_single_hand(&b).unwrap();
        display_hand(&hand);
    }

    #[test]
    fn format_chips_integer() {
        assert_eq!(format_chips(10.0), "10");
        assert_eq!(format_chips(0.0), "0");
    }

    #[test]
    fn format_chips_fractional() {
        assert_eq!(format_chips(10.5), "10.5");
    }

    #[test]
    fn format_bb_works() {
        assert_eq!(format_bb(100.0), "100");
        assert_eq!(format_bb(99.5), "99.5");
        assert_eq!(format_bb(100.001), "100");
    }

    #[test]
    fn position_tag_all() {
        assert_eq!(position_tag(Position::BTN), "BTN");
        assert_eq!(position_tag(Position::SB), "SB");
        assert_eq!(position_tag(Position::BB), "BB");
        assert_eq!(position_tag(Position::EP), "EP");
        assert_eq!(position_tag(Position::MP), "MP");
        assert_eq!(position_tag(Position::CO), "CO");
    }
}
