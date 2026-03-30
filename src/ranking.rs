use std::collections::HashMap;

use crate::card::{self, Card, HandRank};
use crate::parser::{GameData, Hand, Street, went_to_showdown};
use crate::search::hand_pot_bb;

pub struct RankingFilter {
    pub top: usize,
    pub showdown_only: bool,
    pub player: Option<String>,
}

pub struct RankedHand {
    pub hand_number: u32,
    pub hand_id: String,
    pub player_name: String,
    pub hole_cards: Vec<Card>,
    pub board: Vec<Card>,
    pub rank: HandRank,
    pub description: String,
    pub pot_bb: f64,
    pub pot_chips: f64,
    pub at_showdown: bool,
}

fn full_board(hand: &Hand) -> Vec<Card> {
    hand.streets
        .iter()
        .filter(|sd| sd.street != Street::Preflop)
        .flat_map(|sd| sd.new_cards.iter().copied())
        .collect()
}

fn revealed_cards(hand: &Hand) -> HashMap<u8, Vec<Card>> {
    let mut map: HashMap<u8, Vec<Card>> = HashMap::new();

    for (&seat, cards) in &hand.shown_cards {
        if cards.len() >= 2 {
            map.insert(seat, cards.clone());
        }
    }

    for w in &hand.winners {
        if let Some(ref cards) = w.cards
            && cards.len() >= 2
        {
            map.entry(w.seat).or_insert_with(|| cards.clone());
        }
    }

    if hand.real_showdown {
        for p in &hand.players {
            if let Some(ref cards) = p.hole_cards
                && cards.len() >= 2
                && went_to_showdown(hand, p.seat)
            {
                map.entry(p.seat).or_insert_with(|| cards.clone());
            }
        }
    }

    map
}

pub fn rank_hands(data: &GameData, filter: &RankingFilter) -> Vec<RankedHand> {
    let player_lower = filter.player.as_ref().map(|n| n.to_ascii_lowercase());
    let mut results: Vec<RankedHand> = Vec::new();

    for hand in &data.hands {
        let board = full_board(hand);
        if board.len() < 3 {
            continue;
        }

        let revealed = revealed_cards(hand);
        if revealed.is_empty() {
            continue;
        }

        let pot_bb = hand_pot_bb(hand);
        let pot_chips: f64 = hand.winners.iter().map(|w| w.amount).sum();

        for (seat, hole_cards) in &revealed {
            if hole_cards.len() + board.len() < 5 {
                continue;
            }

            let Some(player) = hand.players.iter().find(|p| p.seat == *seat) else {
                continue;
            };

            if let Some(ref target) = player_lower
                && player.name.to_ascii_lowercase() != *target
            {
                continue;
            }

            let at_showdown = hand.real_showdown && went_to_showdown(hand, *seat);

            if filter.showdown_only && !at_showdown {
                continue;
            }

            let mut all_cards = Vec::with_capacity(hole_cards.len() + board.len());
            all_cards.extend_from_slice(hole_cards);
            all_cards.extend_from_slice(&board);
            let rank = card::evaluate(&all_cards);
            let description = card::hand_description(&rank);

            results.push(RankedHand {
                hand_number: hand.number,
                hand_id: hand.id.clone(),
                player_name: player.name.clone(),
                hole_cards: hole_cards.clone(),
                board: board.clone(),
                rank,
                description,
                pot_bb,
                pot_chips,
                at_showdown,
            });
        }
    }

    results.sort_unstable_by(|a, b| {
        b.rank.cmp(&a.rank).then_with(|| a.hand_number.cmp(&b.hand_number))
    });
    results.truncate(filter.top);
    results
}

pub fn print_ranking(results: &[RankedHand], use_chips: bool) {
    if results.is_empty() {
        println!("No shown hands found.");
        return;
    }

    println!("Top {} hands by strength\n", results.len());

    for (i, r) in results.iter().enumerate() {
        let pot_val = if use_chips {
            crate::format_chips(r.pot_chips)
        } else {
            format!("{:.1} BB", r.pot_bb)
        };
        let shown_type = if r.at_showdown { "Showdown" } else { "Voluntary" };
        let hole = r.hole_cards.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
        let board = r.board.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");

        println!(
            "#{:<3} {} — {} (Hand #{}, {})",
            i + 1,
            r.description,
            r.player_name,
            r.hand_number,
            shown_type
        );
        println!("     Hole: [{}]  Board: [{}]  Pot: {}", hole, board, pot_val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_helpers::*;

    fn default_filter() -> RankingFilter {
        RankingFilter {
            top: 10,
            showdown_only: false,
            player: None,
        }
    }

    #[test]
    fn showdown_hands_ranked() {
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kh", "Qh"])
            .check(1)
            .check(2)
            .turn("Jh")
            .check(1)
            .check(2)
            .river("2c")
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["Th", "9h"])
            .show(2, &["3c", "4c"])
            .win(1, 2.0);

        let data = parse_multi_game_data(&[&h]);
        let results = rank_hands(&data, &default_filter());

        assert_eq!(results.len(), 2);
        assert!(results[0].at_showdown);
        assert!(results[1].at_showdown);
        assert!(results[0].rank >= results[1].rank);
        assert!(results[0].description.contains("flush"));
    }

    #[test]
    fn voluntary_show() {
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kh", "Qh"])
            .bet(1, 2.0)
            .fold(2)
            .show(2, &["2c", "3c"])
            .win(1, 4.0);

        let data = parse_multi_game_data(&[&h]);
        let results = rank_hands(&data, &default_filter());

        assert_eq!(results.len(), 1);
        assert!(!results[0].at_showdown);
        assert_eq!(results[0].player_name, "Bob");
    }

    #[test]
    fn showdown_only_filter() {
        let h1 = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kh", "Qh"])
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["Th", "9h"])
            .show(2, &["3c", "4c"])
            .win(1, 2.0);

        let h2 = HandBuilder::new()
            .number(2)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["2d", "3d", "4d"])
            .bet(1, 2.0)
            .fold(2)
            .show(2, &["As", "Ac"])
            .win(1, 4.0);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = RankingFilter {
            showdown_only: true,
            ..default_filter()
        };
        let results = rank_hands(&data, &filter);
        assert!(results.iter().all(|r| r.at_showdown));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn player_filter() {
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kh", "Qh"])
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["Th", "9h"])
            .show(2, &["3c", "4c"])
            .win(1, 2.0);

        let data = parse_multi_game_data(&[&h]);
        let filter = RankingFilter {
            player: Some("alice".into()),
            ..default_filter()
        };
        let results = rank_hands(&data, &filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].player_name, "Alice");
    }

    #[test]
    fn top_truncation() {
        let h1 = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kh", "Qh"])
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["Jh", "Th"])
            .show(2, &["2c", "3c"])
            .win(1, 2.0);

        let h2 = HandBuilder::new()
            .number(2)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["2d", "3d", "4d"])
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["5d", "6d"])
            .show(2, &["7c", "8c"])
            .win(1, 2.0);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = RankingFilter {
            top: 2,
            ..default_filter()
        };
        let results = rank_hands(&data, &filter);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_no_shown_cards() {
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let data = parse_multi_game_data(&[&h]);
        let results = rank_hands(&data, &default_filter());
        assert!(results.is_empty());
    }

    #[test]
    fn no_board_skipped() {
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet(1, 3.0)
            .fold(2)
            .show(2, &["As", "Ah"])
            .win(1, 4.5);

        let data = parse_multi_game_data(&[&h]);
        let results = rank_hands(&data, &default_filter());
        assert!(results.is_empty());
    }

    #[test]
    fn sorted_by_strength() {
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Ad", "Kh"])
            .check(1)
            .check(2)
            .turn("Ks")
            .check(1)
            .check(2)
            .river("2c")
            .check(1)
            .check(2)
            .showdown()
            .show(1, &["Ac", "As"])
            .show(2, &["Kc", "Kd"])
            .win(1, 2.0);

        let data = parse_multi_game_data(&[&h]);
        let results = rank_hands(&data, &default_filter());

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].player_name, "Alice");
        assert_eq!(results[1].player_name, "Bob");
        assert!(results[0].rank > results[1].rank);
    }

    #[test]
    fn winner_cards_included() {
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kh", "Qh"])
            .check(1)
            .check(2)
            .win_with_cards(1, 2.0, &["Jh", "Th"]);

        let data = parse_multi_game_data(&[&h]);
        let results = rank_hands(&data, &default_filter());

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].player_name, "Alice");
    }

    #[test]
    fn print_ranking_no_panic() {
        let results = vec![RankedHand {
            hand_number: 1,
            hand_id: "test123".into(),
            player_name: "Alice".into(),
            hole_cards: vec![Card::parse("Ah").unwrap(), Card::parse("Kh").unwrap()],
            board: vec![
                Card::parse("Qh").unwrap(),
                Card::parse("Jh").unwrap(),
                Card::parse("Th").unwrap(),
            ],
            rank: card::evaluate(&[
                Card::parse("Ah").unwrap(),
                Card::parse("Kh").unwrap(),
                Card::parse("Qh").unwrap(),
                Card::parse("Jh").unwrap(),
                Card::parse("Th").unwrap(),
            ]),
            description: "royal flush".into(),
            pot_bb: 50.0,
            pot_chips: 50.0,
            at_showdown: true,
        }];
        print_ranking(&results, false);
        print_ranking(&results, true);
        print_ranking(&[], false);
    }
}
