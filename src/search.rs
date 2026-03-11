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
