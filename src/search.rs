use crate::parser::{GameData, Hand, Street, net_profit, saw_street, went_to_showdown};

#[derive(Clone, Copy, Default)]
pub enum SortField {
    #[default]
    HandId,
    Pot,
}

pub struct SearchFilter {
    pub player: Option<String>,
    pub saw_flop: Option<String>,
    pub saw_turn: Option<String>,
    pub saw_river: Option<String>,
    pub min_pot: Option<f64>,
    pub max_pot: Option<f64>,
    pub showdown: Option<bool>,
    pub won: bool,
    pub lost: bool,
    pub sort: SortField,
}

pub struct SearchResult {
    pub hand_number: u32,
    pub hand_id: String,
    pub pot_bb: f64,
    pub showdown: bool,
    pub winner_name: String,
    pub winner_amount: f64,
    pub player_net_bb: Option<f64>,
}

pub fn search_hands(data: &GameData, filter: &SearchFilter) -> Vec<SearchResult> {
    let mut results: Vec<SearchResult> = data
        .hands
        .iter()
        .filter(|hand| matches_filter(hand, filter))
        .map(|hand| {
            let pot_bb = hand_pot_bb(hand);
            let (winner_name, winner_amount) = primary_winner(hand);
            let player_net_bb = filter.player.as_ref().map(|name| {
                let seat = find_seat(hand, name).unwrap_or(0);
                if hand.effective_bb > 0.0 {
                    net_profit(hand, seat) / hand.effective_bb
                } else {
                    0.0
                }
            });
            SearchResult {
                hand_number: hand.number,
                hand_id: hand.id.clone(),
                pot_bb,
                showdown: hand.real_showdown,
                winner_name,
                winner_amount,
                player_net_bb,
            }
        })
        .collect();

    match filter.sort {
        SortField::HandId => results.sort_unstable_by_key(|r| r.hand_number),
        SortField::Pot => results.sort_unstable_by(|a, b| b.pot_bb.total_cmp(&a.pot_bb)),
    }

    results
}

fn matches_filter(hand: &Hand, filter: &SearchFilter) -> bool {
    if let Some(ref name) = filter.player {
        let Some(seat) = find_seat(hand, name) else { return false };

        if let Some(want_showdown) = filter.showdown {
            let player_showed = went_to_showdown(hand, seat);
            if want_showdown != player_showed {
                return false;
            }
        }

        if filter.won || filter.lost {
            let net = net_profit(hand, seat);
            if filter.won && net <= 0.0 {
                return false;
            }
            if filter.lost && net >= 0.0 {
                return false;
            }
        }
    } else if let Some(sd) = filter.showdown
        && hand.real_showdown != sd
    {
        return false;
    }

    if let Some(ref name) = filter.saw_flop {
        let Some(seat) = find_seat(hand, name) else { return false };
        if !saw_street(hand, seat, Street::Flop) {
            return false;
        }
    }
    if let Some(ref name) = filter.saw_turn {
        let Some(seat) = find_seat(hand, name) else { return false };
        if !saw_street(hand, seat, Street::Turn) {
            return false;
        }
    }
    if let Some(ref name) = filter.saw_river {
        let Some(seat) = find_seat(hand, name) else { return false };
        if !saw_street(hand, seat, Street::River) {
            return false;
        }
    }
    let pot_bb = hand_pot_bb(hand);
    if let Some(min) = filter.min_pot
        && pot_bb < min
    {
        return false;
    }
    if let Some(max) = filter.max_pot
        && pot_bb > max
    {
        return false;
    }
    true
}

fn primary_winner(hand: &Hand) -> (String, f64) {
    let winner = hand.winners.iter().max_by(|a, b| a.amount.total_cmp(&b.amount));
    match winner {
        Some(w) => {
            let name =
                hand.players.iter().find(|p| p.seat == w.seat).map_or("?", |p| p.name.as_str());
            (name.to_owned(), w.amount)
        }
        None => (String::new(), 0.0),
    }
}

pub fn hand_pot_bb(hand: &Hand) -> f64 {
    if hand.effective_bb <= 0.0 {
        return 0.0;
    }
    let total: f64 = hand.winners.iter().map(|w| w.amount).sum();
    total / hand.effective_bb
}

fn find_seat(hand: &Hand, name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    hand.players.iter().find(|p| p.name.to_ascii_lowercase() == lower).map(|p| p.seat)
}

pub fn print_results(results: &[SearchResult]) {
    println!("Found {} hands matching criteria\n", results.len());
    if results.is_empty() {
        return;
    }
    let has_player_net = results.iter().any(|r| r.player_net_bb.is_some());
    if has_player_net {
        println!(
            "{:<8} {:<17} {:>8}  {:<9} {:<16} {:>8}  {:>10}",
            "Hand #", "ID", "Pot(BB)", "Showdown", "Winner", "Amount", "Player Net"
        );
        println!("{}", "-".repeat(82));
    } else {
        println!(
            "{:<8} {:<17} {:>8}  {:<9} {:<16} {:>8}",
            "Hand #", "ID", "Pot(BB)", "Showdown", "Winner", "Amount"
        );
        println!("{}", "-".repeat(70));
    }
    for r in results {
        let sd = if r.showdown { "Yes" } else { "No" };
        let id_short = if r.hand_id.len() > 16 { &r.hand_id[..16] } else { &r.hand_id };
        if has_player_net {
            let net = r.player_net_bb.map_or_else(String::new, |v| {
                if v >= 0.0 { format!("+{v:.1}") } else { format!("{v:.1}") }
            });
            println!(
                "{:<8} {:<17} {:>8.1}  {:<9} {:<16} {:>8.1}  {:>10}",
                r.hand_number, id_short, r.pot_bb, sd, r.winner_name, r.winner_amount, net
            );
        } else {
            println!(
                "{:<8} {:<17} {:>8.1}  {:<9} {:<16} {:>8.1}",
                r.hand_number, id_short, r.pot_bb, sd, r.winner_name, r.winner_amount
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::parser::test_helpers::*;

    fn default_filter() -> SearchFilter {
        SearchFilter {
            player: None,
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: None,
            won: false,
            lost: false,
            sort: SortField::HandId,
        }
    }

    fn simple_3player_hand(num: u32, pot_winner: u8, win_amount: f64) -> HandBuilder {
        let mut b = HandBuilder::new()
            .number(num)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0);

        if pot_winner == 1 {
            b = b
                .bet(1, 3.0)
                .call(2, 3.0)
                .fold(3)
                .flop(&["Ah", "Kd", "Qs"])
                .check(2)
                .bet(1, 5.0)
                .fold(2)
                .win(1, win_amount);
        } else {
            b = b.fold(1).fold(2).win(pot_winner, win_amount);
        }
        b
    }

    fn showdown_hand(num: u32) -> HandBuilder {
        HandBuilder::new()
            .number(num)
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
            .showdown()
            .show(1, &["9s", "8s"])
            .show(2, &["7s", "6s"])
            .win(1, 2.0)
    }

    #[test]
    fn filter_by_player() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            player: Some("Alice".into()),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn filter_by_saw_flop() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            saw_flop: Some("Alice".into()),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hand_number, 1);
    }

    #[test]
    fn filter_by_min_pot() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            min_pot: Some(5.0),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hand_number, 1);
    }

    #[test]
    fn filter_by_max_pot() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            max_pot: Some(3.0),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hand_number, 2);
    }

    #[test]
    fn filter_by_showdown() {
        let h1 = showdown_hand(1);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);

        let filter_sd = SearchFilter {
            showdown: Some(true),
            ..default_filter()
        };
        let results = search_hands(&data, &filter_sd);
        assert_eq!(results.len(), 1);
        assert!(results[0].showdown);

        let filter_no_sd = SearchFilter {
            showdown: Some(false),
            ..default_filter()
        };
        let results = search_hands(&data, &filter_no_sd);
        assert_eq!(results.len(), 1);
        assert!(!results[0].showdown);
    }

    #[test]
    fn showdown_player_aware() {
        // Alice folds preflop, Bob and Charlie go to showdown
        let h = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(1)
            .call(2, 1.0)
            .check(3)
            .flop(&["Ah", "Kd", "Qs"])
            .check(2)
            .check(3)
            .turn("Js")
            .check(2)
            .check(3)
            .river("Ts")
            .check(2)
            .check(3)
            .showdown()
            .show(2, &["9s", "8s"])
            .show(3, &["7s", "6s"])
            .win(2, 2.0);

        let data = parse_multi_game_data(&[&h]);

        let filter = SearchFilter {
            player: Some("Alice".into()),
            showdown: Some(true),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert!(results.is_empty(), "Alice folded, should not appear in player-aware showdown");

        let filter_bob = SearchFilter {
            player: Some("Bob".into()),
            showdown: Some(true),
            ..default_filter()
        };
        let results = search_hands(&data, &filter_bob);
        assert_eq!(results.len(), 1, "Bob went to showdown");
    }

    #[test]
    fn filter_won_lost() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);

        let filter_won = SearchFilter {
            player: Some("Alice".into()),
            won: true,
            ..default_filter()
        };
        let results = search_hands(&data, &filter_won);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hand_number, 1);

        let filter_lost = SearchFilter {
            player: Some("Alice".into()),
            lost: true,
            ..default_filter()
        };
        let results = search_hands(&data, &filter_lost);
        assert!(results.iter().all(|r| r.player_net_bb.unwrap_or(0.0) < 0.0));
    }

    #[test]
    fn player_net_column() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let data = parse_multi_game_data(&[&h1]);

        let filter = SearchFilter {
            player: Some("Alice".into()),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert!(results[0].player_net_bb.is_some());
    }

    #[test]
    fn sort_by_pot() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            sort: SortField::Pot,
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 2);
        assert!(results[0].pot_bb >= results[1].pot_bb);
    }

    #[test]
    fn sort_by_hand_id() {
        let h2 = simple_3player_hand(2, 3, 1.5);
        let h1 = simple_3player_hand(1, 1, 11.0);

        let data = parse_multi_game_data(&[&h2, &h1]);
        let results = search_hands(&data, &default_filter());
        assert_eq!(results[0].hand_number, 1);
        assert_eq!(results[1].hand_number, 2);
    }

    #[test]
    fn combined_filters() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            player: Some("Alice".into()),
            min_pot: Some(5.0),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hand_number, 1);
    }

    #[test]
    fn no_matches() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let data = parse_multi_game_data(&[&h1]);
        let filter = SearchFilter {
            player: Some("NonexistentPlayer".into()),
            ..default_filter()
        };
        let results = search_hands(&data, &filter);
        assert!(results.is_empty());
    }

    #[test]
    fn hand_pot_bb_calculation() {
        let b = HandBuilder::new()
            .blinds(0.5, 1.0)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .win(1, 2.0);

        let hand = parse_single_hand(&b).unwrap();
        let pot_bb = hand_pot_bb(&hand);
        assert!((pot_bb - 2.0).abs() < 0.001);
    }

    #[test]
    fn hand_pot_bb_zero_bb() {
        let b = HandBuilder::new()
            .blinds(0.0, 0.0)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .win(1, 5.0);

        let hand = parse_single_hand(&b).unwrap();
        assert_eq!(hand_pot_bb(&hand), 0.0);
    }

    #[test]
    fn search_result_has_id() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let data = parse_multi_game_data(&[&h1]);
        let results = search_hands(&data, &default_filter());
        assert!(!results[0].hand_id.is_empty());
    }

    #[test]
    fn print_results_no_panic() {
        let results = vec![SearchResult {
            hand_number: 1,
            hand_id: "test123".into(),
            pot_bb: 5.0,
            showdown: true,
            winner_name: "Alice".into(),
            winner_amount: 5.0,
            player_net_bb: Some(3.0),
        }];
        print_results(&results);
        print_results(&[]);
    }

    #[test]
    fn player_saw_street_detection() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .call(2, 3.0)
            .fold(3)
            .flop(&["Ah", "Kd", "Qs"])
            .check(2)
            .bet(1, 5.0)
            .fold(2)
            .win(1, 11.0);

        let hand = parse_single_hand(&b).unwrap();
        assert!(saw_street(&hand, 1, Street::Flop));
        assert!(saw_street(&hand, 2, Street::Flop));
        assert!(!saw_street(&hand, 3, Street::Flop));
    }
}
