use std::collections::HashMap;

use crate::card::{self, Card};
use crate::parser::{ActionType, Hand, Position, Street, net_profit};

const POSITION_ORDER: [Position; 6] =
    [Position::BTN, Position::SB, Position::BB, Position::EP, Position::MP, Position::CO];

pub fn display_hand(hand: &Hand, use_chips: bool) {
    let seat_name = build_seat_name_map(hand);
    let hole_cards = build_hole_cards_map(hand);
    let bb = hand.effective_bb;

    print_header(hand, &seat_name, bb, use_chips);
    print_players(hand, &hole_cards, bb, use_chips);

    let mut board: Vec<Card> = Vec::new();
    let mut running_pot = 0.0;

    for (street_idx, sd) in hand.streets.iter().enumerate() {
        if hand.run_it_twice && sd.street != Street::Preflop && sd.actions.is_empty() {
            break;
        }

        print_street_header(sd.street, None, &sd.new_cards, &board, running_pot, bb, use_chips);

        if sd.street != Street::Preflop {
            board.extend_from_slice(&sd.new_cards);
        }

        if sd.street != Street::Preflop {
            let folded = collect_folded_seats(hand, street_idx);
            print_made_hands(&hole_cards, &board, &seat_name, &folded);
        }

        running_pot = print_actions(&sd.actions, &seat_name, running_pot, bb, use_chips);
    }

    if hand.run_it_twice {
        print_run_it_twice(hand, &seat_name, &hole_cards, &board, running_pot, bb, use_chips);
    } else {
        print_results(hand, &seat_name, bb, use_chips);
    }

    print_net_pnl(hand, &seat_name, bb, use_chips);
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

fn print_header(hand: &Hand, seat_name: &HashMap<u8, String>, bb: f64, use_chips: bool) {
    let bomb = if hand.bomb_pot { " [BOMB POT]" } else { "" };
    let straddle_tag = if hand.straddle_seat.is_some() {
        format!(" [STR {}]", format_chips(hand.effective_bb))
    } else {
        String::new()
    };
    let eff_stack = hand.players.iter().map(|p| p.stack).fold(f64::INFINITY, f64::min);
    let eff_display = if use_chips {
        format_chips(eff_stack)
    } else if bb > 0.0 {
        format!("{} BB", format_bb(eff_stack / bb))
    } else {
        format_chips(eff_stack)
    };
    println!(
        "Hand #{} ({}) | Stakes {}/{} | {} players | Eff: {}{}{}",
        hand.number,
        hand.id,
        format_chips(hand.small_blind),
        format_chips(hand.big_blind),
        hand.players.len(),
        eff_display,
        straddle_tag,
        bomb,
    );

    if let Some(dealer) = hand.players.iter().find(|p| p.position == Position::BTN) {
        let name = seat_name.get(&dealer.seat).map_or("?", String::as_str);
        println!("Dealer: {} (BTN)", name);
    }

    println!();
}

fn print_players(hand: &Hand, hole_cards: &HashMap<u8, Vec<Card>>, bb: f64, use_chips: bool) {
    println!("Players:");

    let mut sorted: Vec<_> = hand.players.iter().collect();
    sorted.sort_by_key(|p| {
        POSITION_ORDER.iter().position(|&pos| pos == p.position).unwrap_or(usize::MAX)
    });

    for p in sorted {
        let pos_str = format!("{:4}", position_tag(p.position));
        let stack_display = if use_chips {
            format_chips(p.stack)
        } else if bb > 0.0 {
            format!("{} BB", format_bb(p.stack / bb))
        } else {
            format_chips(p.stack)
        };
        let cards_str = match hole_cards.get(&p.seat) {
            Some(cards) => format!("  [{}]", format_cards(cards)),
            None => String::new(),
        };
        println!("  {}{:<8} {}{}", pos_str, p.name, stack_display, cards_str);
    }

    println!();
}

fn print_street_header(
    street: Street,
    run: Option<u8>,
    new_cards: &[Card],
    board: &[Card],
    pot: f64,
    bb: f64,
    use_chips: bool,
) {
    let street_name = match street {
        Street::Preflop => "PREFLOP",
        Street::Flop => "FLOP",
        Street::Turn => "TURN",
        Street::River => "RIVER",
    };
    let run_label = match run {
        Some(r) => format!(" {r}"),
        None => String::new(),
    };
    let pot_str = format_val(pot, bb, use_chips);

    let header = match street {
        Street::Preflop => format!("=== {street_name} === (pot: {pot_str})"),
        Street::Flop => {
            format!(
                "=== {street_name}{run_label} [{}] === (pot: {pot_str})",
                format_cards(new_cards),
            )
        }
        Street::Turn | Street::River => {
            format!(
                "=== {street_name}{run_label} [{}] [{}] === (pot: {pot_str})",
                format_cards(board),
                format_cards(new_cards),
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
    pot_before_street: f64,
    bb: f64,
    use_chips: bool,
) -> f64 {
    let mut pot = pot_before_street;
    let mut current_bet: f64 = 0.0;
    let mut per_seat: HashMap<u8, f64> = HashMap::new();

    for a in actions {
        let name = seat_name.get(&a.seat).map_or("?", String::as_str);
        let all_in_tag = if a.all_in { " (all-in)" } else { "" };
        let seat_invested = *per_seat.get(&a.seat).unwrap_or(&0.0);

        let line = match a.kind {
            ActionType::SmallBlind => {
                pot += a.amount - seat_invested;
                per_seat.insert(a.seat, a.amount);
                current_bet = current_bet.max(a.amount);
                format!("  {} posts small blind {}", name, format_val(a.amount, bb, use_chips))
            }
            ActionType::BigBlind => {
                pot += a.amount - seat_invested;
                per_seat.insert(a.seat, a.amount);
                current_bet = current_bet.max(a.amount);
                format!("  {} posts big blind {}", name, format_val(a.amount, bb, use_chips))
            }
            ActionType::Ante => {
                pot += a.amount;
                format!("  {} antes {}", name, format_val(a.amount, bb, use_chips))
            }
            ActionType::Straddle => {
                pot += a.amount - seat_invested;
                per_seat.insert(a.seat, a.amount);
                current_bet = current_bet.max(a.amount);
                format!("  {} straddles {}", name, format_val(a.amount, bb, use_chips))
            }
            ActionType::DeadBlind => {
                pot += a.amount;
                format!("  {} posts dead blind {}", name, format_val(a.amount, bb, use_chips))
            }
            ActionType::Fold => format!("  {name} folds"),
            ActionType::Check => format!("  {name} checks"),
            ActionType::Call => {
                pot += a.amount - seat_invested;
                per_seat.insert(a.seat, a.amount);
                format!("  {} calls {}{}", name, format_val(a.amount, bb, use_chips), all_in_tag)
            }
            ActionType::Bet => {
                let is_raise = current_bet > 0.0;
                let sizing = if is_raise {
                    let call_amount = current_bet - seat_invested;
                    let increment = a.amount - current_bet;
                    let denominator = pot + call_amount;
                    if denominator > 0.0 {
                        format!(" ({}% pot)", (increment / denominator * 100.0).round() as i64)
                    } else {
                        String::new()
                    }
                } else if pot > 0.0 {
                    format!(" ({}% pot)", (a.amount / pot * 100.0).round() as i64)
                } else {
                    String::new()
                };
                let verb = if is_raise { "raises to" } else { "bets" };
                pot += a.amount - seat_invested;
                per_seat.insert(a.seat, a.amount);
                current_bet = a.amount;
                format!(
                    "  {} {} {}{}{}",
                    name,
                    verb,
                    format_val(a.amount, bb, use_chips),
                    sizing,
                    all_in_tag
                )
            }
        };
        println!("{line}");
    }

    println!();
    pot
}

fn print_run_it_twice(
    hand: &Hand,
    seat_name: &HashMap<u8, String>,
    hole_cards: &HashMap<u8, Vec<Card>>,
    shared_board: &[Card],
    pot: f64,
    bb: f64,
    use_chips: bool,
) {
    let rit_streets: Vec<&crate::parser::StreetData> = hand
        .streets
        .iter()
        .filter(|sd| sd.street != Street::Preflop && sd.actions.is_empty())
        .collect();

    println!("Players choose to run it twice");
    println!();

    for run_num in 1..=2u8 {
        println!("=== RUN {run_num} ===");
        let mut board = shared_board.to_vec();

        for sd in &rit_streets {
            let new_cards = if run_num == 2 {
                hand.run2_cards
                    .iter()
                    .find(|(s, _)| *s == sd.street)
                    .map_or(sd.new_cards.as_slice(), |(_, c)| c.as_slice())
            } else {
                &sd.new_cards
            };

            print_street_header(sd.street, Some(run_num), new_cards, &board, pot, bb, use_chips);
            board.extend_from_slice(new_cards);
            print_made_hands(hole_cards, &board, seat_name, &[]);
        }

        let run_winners: Vec<_> = hand.winners.iter().filter(|w| w.run == run_num).collect();
        if !run_winners.is_empty() {
            println!();
            println!("Result (Run {run_num}):");
            for w in &run_winners {
                let name = seat_name.get(&w.seat).map_or("?", String::as_str);
                let amt = format_val(w.amount, bb, use_chips);
                match &w.hand_description {
                    Some(desc) => println!("  {name} wins {amt} ({desc})"),
                    None => println!("  {name} wins {amt}"),
                }
            }
        }
        println!();
    }
}

fn print_results(hand: &Hand, seat_name: &HashMap<u8, String>, bb: f64, use_chips: bool) {
    if hand.winners.is_empty() {
        return;
    }

    println!("Result:");
    for w in &hand.winners {
        let name = seat_name.get(&w.seat).map_or("?", String::as_str);
        let amt = format_val(w.amount, bb, use_chips);
        match &w.hand_description {
            Some(desc) => println!("  {name} wins {amt} ({desc})"),
            None => println!("  {name} wins {amt}"),
        }
    }
}

fn print_net_pnl(hand: &Hand, seat_name: &HashMap<u8, String>, bb: f64, use_chips: bool) {
    let mut entries: Vec<(&str, f64)> = hand
        .players
        .iter()
        .map(|p| {
            let name = seat_name.get(&p.seat).map_or("?", String::as_str);
            let net = net_profit(hand, p.seat);
            (name, net)
        })
        .filter(|(_, net)| net.abs() > 0.001)
        .collect();

    if entries.is_empty() {
        return;
    }

    entries.sort_by(|a, b| b.1.total_cmp(&a.1));

    let parts: Vec<String> = entries
        .iter()
        .map(|(name, net)| {
            let val = format_val(net.abs(), bb, use_chips);
            if *net >= 0.0 { format!("{name} +{val}") } else { format!("{name} -{val}") }
        })
        .collect();

    println!("Net: {}", parts.join(" | "));
}

fn format_cards(cards: &[Card]) -> String {
    cards.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
}

fn format_chips(amount: f64) -> String {
    crate::format_chips(amount)
}

fn format_val(amount: f64, bb: f64, use_chips: bool) -> String {
    if use_chips {
        format_chips(amount)
    } else if bb > 0.0 {
        format_bb(amount / bb)
    } else {
        format_chips(amount)
    }
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
        display_hand(&hand, false);
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
        display_hand(&hand, false);
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
        display_hand(&hand, false);
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
        display_hand(&hand, false);
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
        display_hand(&hand, false);
    }

    #[test]
    fn display_run_it_twice() {
        let b = HandBuilder::new()
            .player_with_hand("p1", 1, "Alice", 100.0, &["As", "Kd"])
            .player_with_hand("p2", 2, "Bob", 50.0, &["Qh", "Qd"])
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet_all_in(1, 100.0)
            .call_all_in(2, 50.0)
            .uncalled_return(1, 50.0)
            .rit_vote()
            .flop(&["Ah", "Kh", "Qs"])
            .board_run2(1, &["Qc", "Jd", "Ts"])
            .turn("Js")
            .board_run2(2, &["9h"])
            .river("Ts")
            .board_run2(3, &["8h"])
            .showdown()
            .show(1, &["As", "Kd"])
            .show(2, &["Qh", "Qd"])
            .win_run(1, 50.0, 1)
            .win_run(2, 50.0, 2);

        let hand = parse_single_hand(&b).unwrap();
        assert!(hand.run_it_twice);
        display_hand(&hand, false);
    }

    #[test]
    fn display_chips_mode() {
        let b = HandBuilder::new()
            .player_with_hand("p1", 1, "Alice", 100.0, &["As", "Kd"])
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet(1, 3.0)
            .fold(2)
            .win(1, 4.5);

        let hand = parse_single_hand(&b).unwrap();
        display_hand(&hand, true);
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
    fn format_val_bb_mode() {
        assert_eq!(format_val(10.0, 2.0, false), "5");
        assert_eq!(format_val(5.0, 2.0, false), "2.5");
    }

    #[test]
    fn format_val_chips_mode() {
        assert_eq!(format_val(10.0, 2.0, true), "10");
        assert_eq!(format_val(5.5, 2.0, true), "5.5");
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
