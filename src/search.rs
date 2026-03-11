use crate::parser::{ActionType, GameData, Hand, Street};

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
    pub sort: SortField,
}

pub struct SearchResult {
    pub hand_number: u32,
    pub pot_bb: f64,
    pub showdown: bool,
    pub winner_name: String,
    pub winner_amount: f64,
}

pub fn search_hands(data: &GameData, filter: &SearchFilter) -> Vec<SearchResult> {
    let mut results: Vec<SearchResult> = data
        .hands
        .iter()
        .filter(|hand| matches_filter(hand, filter))
        .map(|hand| {
            let pot_bb = hand_pot_bb(hand);
            let (winner_name, winner_amount) = primary_winner(hand);
            SearchResult {
                hand_number: hand.number,
                pot_bb,
                showdown: hand.real_showdown,
                winner_name,
                winner_amount,
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
    if let Some(ref name) = filter.player
        && !player_vpipped(hand, name)
    {
        return false;
    }
    if let Some(ref name) = filter.saw_flop
        && !player_saw_street(hand, name, Street::Flop)
    {
        return false;
    }
    if let Some(ref name) = filter.saw_turn
        && !player_saw_street(hand, name, Street::Turn)
    {
        return false;
    }
    if let Some(ref name) = filter.saw_river
        && !player_saw_street(hand, name, Street::River)
    {
        return false;
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
    if let Some(sd) = filter.showdown
        && hand.real_showdown != sd
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

pub fn player_vpipped(hand: &Hand, name: &str) -> bool {
    let Some(seat) = find_seat(hand, name) else { return false };
    let preflop = match hand.streets.first() {
        Some(sd) if sd.street == Street::Preflop => sd,
        _ => return false,
    };
    preflop
        .actions
        .iter()
        .any(|a| a.seat == seat && matches!(a.kind, ActionType::Call | ActionType::Bet))
}

pub fn player_saw_street(hand: &Hand, name: &str, street: Street) -> bool {
    let Some(seat) = find_seat(hand, name) else { return false };

    if !hand.streets.iter().any(|sd| sd.street == street) {
        return false;
    }

    for sd in &hand.streets {
        if sd.street >= street {
            break;
        }
        if sd.actions.iter().any(|a| a.seat == seat && a.kind == ActionType::Fold) {
            return false;
        }
    }

    let has_action_on_or_after = hand
        .streets
        .iter()
        .any(|sd| sd.street >= street && sd.actions.iter().any(|a| a.seat == seat));
    let is_winner = hand.winners.iter().any(|w| w.seat == seat);

    has_action_on_or_after || is_winner
}

pub fn hand_pot_bb(hand: &Hand) -> f64 {
    if hand.big_blind <= 0.0 {
        return 0.0;
    }
    let total: f64 = hand.winners.iter().map(|w| w.amount).sum();
    total / hand.big_blind
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
    println!(
        "{:<8} {:>8}  {:<9} {:<16} {:>8}",
        "Hand #", "Pot(BB)", "Showdown", "Winner", "Amount"
    );
    println!("{}", "-".repeat(56));
    for r in results {
        let sd = if r.showdown { "Yes" } else { "No" };
        println!(
            "{:<8} {:>8.1}  {:<9} {:<16} {:>8.1}",
            r.hand_number, r.pot_bb, sd, r.winner_name, r.winner_amount
        );
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::parser::test_helpers::*;

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
    fn filter_by_player_vpip() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            player: Some("Alice".into()),
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: None,
            sort: SortField::HandId,
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hand_number, 1);
    }

    #[test]
    fn filter_by_saw_flop() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            player: None,
            saw_flop: Some("Alice".into()),
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: None,
            sort: SortField::HandId,
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
            player: None,
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: Some(5.0),
            max_pot: None,
            showdown: None,
            sort: SortField::HandId,
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
            player: None,
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: Some(3.0),
            showdown: None,
            sort: SortField::HandId,
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
            player: None,
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: Some(true),
            sort: SortField::HandId,
        };
        let results = search_hands(&data, &filter_sd);
        assert_eq!(results.len(), 1);
        assert!(results[0].showdown);

        let filter_no_sd = SearchFilter {
            player: None,
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: Some(false),
            sort: SortField::HandId,
        };
        let results = search_hands(&data, &filter_no_sd);
        assert_eq!(results.len(), 1);
        assert!(!results[0].showdown);
    }

    #[test]
    fn sort_by_pot() {
        let h1 = simple_3player_hand(1, 1, 11.0);
        let h2 = simple_3player_hand(2, 3, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let filter = SearchFilter {
            player: None,
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: None,
            sort: SortField::Pot,
        };
        let results = search_hands(&data, &filter);
        assert_eq!(results.len(), 2);
        assert!(results[0].pot_bb >= results[1].pot_bb);
    }

    #[test]
    fn sort_by_hand_id() {
        let h2 = simple_3player_hand(2, 3, 1.5);
        let h1 = simple_3player_hand(1, 1, 11.0);

        // Insert h2 before h1 to test sorting
        let data = parse_multi_game_data(&[&h2, &h1]);
        let filter = SearchFilter {
            player: None,
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: None,
            sort: SortField::HandId,
        };
        let results = search_hands(&data, &filter);
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
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: Some(5.0),
            max_pot: None,
            showdown: None,
            sort: SortField::HandId,
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
            saw_flop: None,
            saw_turn: None,
            saw_river: None,
            min_pot: None,
            max_pot: None,
            showdown: None,
            sort: SortField::HandId,
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
    fn print_results_no_panic() {
        let results = vec![SearchResult {
            hand_number: 1,
            pot_bb: 5.0,
            showdown: true,
            winner_name: "Alice".into(),
            winner_amount: 5.0,
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
        assert!(player_saw_street(&hand, "Alice", Street::Flop));
        assert!(player_saw_street(&hand, "Bob", Street::Flop));
        assert!(!player_saw_street(&hand, "Charlie", Street::Flop));
    }
}
