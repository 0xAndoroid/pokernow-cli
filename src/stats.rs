use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::card::{Card, evaluate};
use crate::parser::{ActionType, GameData, Hand, Position, Street, StreetData, net_profit};

#[derive(Default)]
pub struct PlayerStats {
    pub player_id: String,
    pub name: String,
    pub hands_played: u32,
    pub hands_at_table: u32,

    pub vpip_hands: u32,
    pub pfr_hands: u32,
    pub three_bet_opp: u32,
    pub three_bets: u32,
    pub fold_to_three_bet_opp: u32,
    pub fold_to_three_bets: u32,
    pub cold_call_opp: u32,
    pub cold_calls: u32,
    pub open_raises: u32,
    pub limps: u32,

    pub cbet_opp: u32,
    pub cbets: u32,
    pub fold_to_cbet_opp: u32,
    pub fold_to_cbets: u32,
    pub postflop_bets: u32,
    pub postflop_calls: u32,

    pub saw_flop: u32,
    pub won_when_saw_flop: u32,
    pub went_to_showdown: u32,
    pub won_at_showdown: u32,

    pub net_bb: f64,

    pub pos_vpip: [u32; 6],
    pub pos_pfr: [u32; 6],
    pub pos_hands: [u32; 6],

    pub all_in_ev_diff: f64,
    pub all_in_hands: u32,
}

fn pos_index(pos: Position) -> usize {
    match pos {
        Position::EP => 0,
        Position::MP => 1,
        Position::CO => 2,
        Position::BTN => 3,
        Position::SB => 4,
        Position::BB => 5,
    }
}

fn fold_street(hand: &Hand, seat: u8) -> Option<Street> {
    for sd in &hand.streets {
        for a in &sd.actions {
            if a.seat == seat && a.kind == ActionType::Fold {
                return Some(sd.street);
            }
        }
    }
    None
}

fn saw_street(hand: &Hand, seat: u8, street: Street) -> bool {
    if let Some(fs) = fold_street(hand, seat) {
        return fs >= street;
    }
    if hand.winners.iter().any(|w| w.seat == seat) {
        return true;
    }
    for sd in &hand.streets {
        if sd.street >= street {
            for a in &sd.actions {
                if a.seat == seat {
                    return true;
                }
            }
        }
    }
    street == Street::Preflop
}

fn is_forced(at: ActionType) -> bool {
    matches!(
        at,
        ActionType::SmallBlind
            | ActionType::BigBlind
            | ActionType::Ante
            | ActionType::Straddle
            | ActionType::DeadBlind
    )
}

fn preflop_aggressor(preflop: &StreetData) -> Option<u8> {
    let mut last = None;
    for a in &preflop.actions {
        if a.kind == ActionType::Bet {
            last = Some(a.seat);
        }
    }
    last
}

fn board_at_street(hand: &Hand, street: Street) -> Vec<Card> {
    let mut board = Vec::with_capacity(5);
    for sd in &hand.streets {
        if sd.street > Street::Preflop && sd.street <= street {
            board.extend_from_slice(&sd.new_cards);
        }
    }
    board
}

fn went_to_showdown(hand: &Hand, seat: u8) -> bool {
    if !hand.real_showdown {
        return false;
    }
    if hand.shown_cards.contains_key(&seat) {
        return true;
    }
    hand.winners.iter().any(|w| w.seat == seat && w.cards.is_some())
}

fn get_hole_cards(hand: &Hand, seat: u8) -> Option<&Vec<Card>> {
    hand.players.iter().find(|p| p.seat == seat).and_then(|p| p.hole_cards.as_ref())
}

pub fn compute_stats(data: &GameData) -> Vec<PlayerStats> {
    let mut map: HashMap<&str, PlayerStats> = HashMap::new();

    for hand in &data.hands {
        for p in &hand.players {
            let stats = map.entry(p.id.as_str()).or_insert_with(|| PlayerStats {
                player_id: p.id.clone(),
                name: data.player_names.get(&p.id).cloned().unwrap_or_else(|| p.name.clone()),
                ..PlayerStats::default()
            });

            stats.hands_at_table += 1;
            stats.net_bb += net_profit(hand, p.seat) / hand.big_blind;

            if !hand.bomb_pot {
                stats.hands_played += 1;
                let idx = pos_index(p.position);
                stats.pos_hands[idx] += 1;
            }
        }

        process_preflop(hand, &mut map);
        process_postflop(hand, &mut map);
        process_showdown(hand, &mut map);
        process_all_in_ev(hand, &mut map);
    }

    let mut result: Vec<PlayerStats> = map.into_values().collect();
    result.sort_unstable_by(|a, b| b.net_bb.partial_cmp(&a.net_bb).unwrap());
    result
}

fn process_preflop(hand: &Hand, map: &mut HashMap<&str, PlayerStats>) {
    if hand.bomb_pot {
        return;
    }

    let preflop = match hand.streets.first() {
        Some(sd) if sd.street == Street::Preflop => sd,
        _ => return,
    };

    let seat_to_id: HashMap<u8, &str> =
        hand.players.iter().map(|p| (p.seat, p.id.as_str())).collect();

    let seat_to_pos: HashMap<u8, Position> =
        hand.players.iter().map(|p| (p.seat, p.position)).collect();

    let mut raise_count: u32 = 0;
    let mut open_raiser: Option<u8> = None;
    let mut has_voluntarily_acted: HashSet<u8> = HashSet::new();
    let mut folded: HashSet<u8> = HashSet::new();
    let mut three_bettor: Option<u8> = None;
    let mut vpip_seats: HashSet<u8> = HashSet::new();
    let mut pfr_seats: HashSet<u8> = HashSet::new();

    let mut faced_open_raise: HashSet<u8> = HashSet::new();

    for a in &preflop.actions {
        if is_forced(a.kind) {
            continue;
        }

        let seat = a.seat;
        let id = match seat_to_id.get(&seat) {
            Some(id) => *id,
            None => continue,
        };

        match a.kind {
            ActionType::Fold => {
                if raise_count == 1 && open_raiser != Some(seat) && !folded.contains(&seat) {
                    faced_open_raise.insert(seat);
                }
                if raise_count == 2
                    && three_bettor.is_some()
                    && Some(seat) == open_raiser
                    && let Some(stats) = map.get_mut(id)
                {
                    stats.fold_to_three_bet_opp += 1;
                    stats.fold_to_three_bets += 1;
                }
                folded.insert(seat);
            }
            ActionType::Call => {
                vpip_seats.insert(seat);
                if raise_count == 0 {
                    if let Some(stats) = map.get_mut(id) {
                        stats.limps += 1;
                    }
                } else if raise_count >= 1 && !has_voluntarily_acted.contains(&seat) {
                    let is_bb = seat_to_pos.get(&seat) == Some(&Position::BB);
                    if raise_count == 1
                        && !is_bb
                        && let Some(stats) = map.get_mut(id)
                    {
                        stats.cold_call_opp += 1;
                        stats.cold_calls += 1;
                    }
                    if raise_count == 1 && open_raiser != Some(seat) {
                        faced_open_raise.insert(seat);
                    }
                } else if raise_count == 1 && open_raiser != Some(seat) {
                    faced_open_raise.insert(seat);
                }

                if raise_count == 2
                    && three_bettor.is_some()
                    && Some(seat) == open_raiser
                    && let Some(stats) = map.get_mut(id)
                {
                    stats.fold_to_three_bet_opp += 1;
                }

                has_voluntarily_acted.insert(seat);
            }
            ActionType::Bet => {
                vpip_seats.insert(seat);
                pfr_seats.insert(seat);
                raise_count += 1;

                if raise_count == 1 {
                    open_raiser = Some(seat);
                    if let Some(stats) = map.get_mut(id) {
                        stats.open_raises += 1;
                    }
                } else if raise_count == 2 {
                    three_bettor = Some(seat);
                    if open_raiser != Some(seat) {
                        faced_open_raise.insert(seat);
                    }
                    if let Some(stats) = map.get_mut(id) {
                        stats.three_bets += 1;
                    }
                }

                if raise_count == 2
                    && !has_voluntarily_acted.contains(&seat)
                    && seat_to_pos.get(&seat) != Some(&Position::BB)
                    && let Some(stats) = map.get_mut(id)
                {
                    stats.cold_call_opp += 1;
                }

                has_voluntarily_acted.insert(seat);
            }
            _ => {}
        }
    }

    for &seat in &faced_open_raise {
        let id = match seat_to_id.get(&seat) {
            Some(id) => *id,
            None => continue,
        };
        if let Some(stats) = map.get_mut(id) {
            stats.three_bet_opp += 1;
        }
    }
    if let Some(tb) = three_bettor
        && !faced_open_raise.contains(&tb)
    {
        let id = match seat_to_id.get(&tb) {
            Some(id) => *id,
            None => return,
        };
        if let Some(stats) = map.get_mut(id) {
            stats.three_bet_opp += 1;
        }
    }

    for &seat in &vpip_seats {
        let id = match seat_to_id.get(&seat) {
            Some(id) => *id,
            None => continue,
        };
        if let Some(stats) = map.get_mut(id) {
            stats.vpip_hands += 1;
            if let Some(&pos) = seat_to_pos.get(&seat) {
                stats.pos_vpip[pos_index(pos)] += 1;
            }
        }
    }
    for &seat in &pfr_seats {
        let id = match seat_to_id.get(&seat) {
            Some(id) => *id,
            None => continue,
        };
        if let Some(stats) = map.get_mut(id) {
            stats.pfr_hands += 1;
            if let Some(&pos) = seat_to_pos.get(&seat) {
                stats.pos_pfr[pos_index(pos)] += 1;
            }
        }
    }
}

fn process_postflop(hand: &Hand, map: &mut HashMap<&str, PlayerStats>) {
    let seat_to_id: HashMap<u8, &str> =
        hand.players.iter().map(|p| (p.seat, p.id.as_str())).collect();

    let preflop = match hand.streets.first() {
        Some(sd) if sd.street == Street::Preflop => sd,
        _ => return,
    };

    let pf_aggressor = if hand.bomb_pot { None } else { preflop_aggressor(preflop) };

    let winner_seats: HashSet<u8> = hand.winners.iter().map(|w| w.seat).collect();

    for p in &hand.players {
        if saw_street(hand, p.seat, Street::Flop)
            && hand.streets.iter().any(|sd| sd.street >= Street::Flop)
            && let Some(stats) = map.get_mut(p.id.as_str())
        {
            stats.saw_flop += 1;
            if winner_seats.contains(&p.seat) {
                stats.won_when_saw_flop += 1;
            }
        }
    }

    for sd in &hand.streets {
        if sd.street == Street::Preflop {
            continue;
        }

        let mut first_bettor: Option<u8> = None;

        for a in &sd.actions {
            let id = match seat_to_id.get(&a.seat) {
                Some(id) => *id,
                None => continue,
            };

            match a.kind {
                ActionType::Bet => {
                    if let Some(stats) = map.get_mut(id) {
                        stats.postflop_bets += 1;
                    }
                    if first_bettor.is_none() {
                        first_bettor = Some(a.seat);

                        if sd.street == Street::Flop
                            && Some(a.seat) == pf_aggressor
                            && let Some(stats) = map.get_mut(id)
                        {
                            stats.cbets += 1;
                        }
                    }
                }
                ActionType::Call => {
                    if let Some(stats) = map.get_mut(id) {
                        stats.postflop_calls += 1;
                    }
                }
                ActionType::Fold => {
                    if sd.street == Street::Flop
                        && first_bettor.is_some()
                        && first_bettor == pf_aggressor
                        && let Some(stats) = map.get_mut(id)
                    {
                        stats.fold_to_cbet_opp += 1;
                        stats.fold_to_cbets += 1;
                    }
                }
                _ => {}
            }
        }

        if sd.street == Street::Flop {
            if let Some(agg) = pf_aggressor
                && saw_street(hand, agg, Street::Flop)
                && let Some(id) = seat_to_id.get(&agg)
                && let Some(stats) = map.get_mut(*id)
            {
                stats.cbet_opp += 1;
            }

            if let Some(cbet_seat) = first_bettor
                && Some(cbet_seat) == pf_aggressor
            {
                for a in &sd.actions {
                    if a.seat == cbet_seat {
                        continue;
                    }
                    match a.kind {
                        ActionType::Call | ActionType::Bet => {
                            if let Some(id) = seat_to_id.get(&a.seat)
                                && let Some(stats) = map.get_mut(*id)
                            {
                                stats.fold_to_cbet_opp += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn process_showdown(hand: &Hand, map: &mut HashMap<&str, PlayerStats>) {
    if !hand.real_showdown {
        return;
    }

    let winner_seats: HashSet<u8> = hand.winners.iter().map(|w| w.seat).collect();

    for p in &hand.players {
        if !saw_street(hand, p.seat, Street::Flop) {
            continue;
        }
        if !hand.streets.iter().any(|sd| sd.street >= Street::Flop) {
            continue;
        }
        if went_to_showdown(hand, p.seat)
            && let Some(stats) = map.get_mut(p.id.as_str())
        {
            stats.went_to_showdown += 1;
            if winner_seats.contains(&p.seat) {
                stats.won_at_showdown += 1;
            }
        }
    }
}

fn process_all_in_ev(hand: &Hand, map: &mut HashMap<&str, PlayerStats>) {
    if !hand.real_showdown {
        return;
    }

    let mut all_in_seats: HashSet<u8> = HashSet::new();
    let mut all_in_street: Option<Street> = None;
    for sd in &hand.streets {
        for a in &sd.actions {
            if a.all_in {
                all_in_seats.insert(a.seat);
                if all_in_street.is_none() {
                    all_in_street = Some(sd.street);
                }
            }
        }
    }

    if all_in_seats.len() < 2 {
        return;
    }

    let Some(ai_street) = all_in_street else { return };

    let mut known: Vec<(u8, &Vec<Card>)> = Vec::new();
    for &seat in &all_in_seats {
        if let Some(cards) = get_hole_cards(hand, seat)
            && cards.len() == 2
        {
            known.push((seat, cards));
        }
    }

    if known.len() < 2 {
        return;
    }

    let board = board_at_street(hand, ai_street);

    let total_won: f64 = hand.winners.iter().map(|w| w.amount).sum();
    let total_returned: f64 = hand.uncalled_returns.values().sum();
    let pot = total_won + total_returned;

    let equities = calculate_multi_equity(&known, &board, hand);

    let seat_to_id: HashMap<u8, &str> =
        hand.players.iter().map(|p| (p.seat, p.id.as_str())).collect();

    for (i, &(seat, _)) in known.iter().enumerate() {
        let actual: f64 =
            hand.winners.iter().filter(|w| w.seat == seat).map(|w| w.amount).sum::<f64>()
                + hand.uncalled_returns.get(&seat).copied().unwrap_or(0.0);

        let ev_diff = equities[i] * pot - actual;

        if let Some(id) = seat_to_id.get(&seat)
            && let Some(stats) = map.get_mut(*id)
        {
            stats.all_in_ev_diff += ev_diff;
            stats.all_in_hands += 1;
        }
    }
}

fn calculate_multi_equity(players: &[(u8, &Vec<Card>)], board: &[Card], hand: &Hand) -> Vec<f64> {
    let cards_needed = 5 - board.len();
    if cards_needed == 0 {
        return evaluate_final(players, board);
    }

    let mut dead: HashSet<Card> = HashSet::new();
    for (_, cards) in players {
        for &c in *cards {
            dead.insert(c);
        }
    }
    for &c in board {
        dead.insert(c);
    }
    for cards in hand.shown_cards.values() {
        for &c in cards {
            dead.insert(c);
        }
    }

    let deck: Vec<Card> = build_remaining_deck(&dead);

    if cards_needed <= 2 {
        enumerate_equity(players, board, &deck, cards_needed)
    } else {
        monte_carlo_equity(players, board, &deck, cards_needed, 10_000)
    }
}

fn build_remaining_deck(dead: &HashSet<Card>) -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);
    for rank in 2..=14u8 {
        for suit in 0..4u8 {
            let c = Card::new(rank, suit);
            if !dead.contains(&c) {
                deck.push(c);
            }
        }
    }
    deck
}

fn evaluate_final(players: &[(u8, &Vec<Card>)], board: &[Card]) -> Vec<f64> {
    let n = players.len();
    let mut equities = vec![0.0; n];

    let mut ranks = Vec::with_capacity(n);
    for (_, hole) in players {
        let mut combined = Vec::with_capacity(hole.len() + board.len());
        combined.extend_from_slice(hole);
        combined.extend_from_slice(board);
        ranks.push(evaluate(&combined));
    }

    let best = ranks.iter().map(|r| r.score).max().unwrap_or(0);
    let winners: Vec<usize> =
        ranks.iter().enumerate().filter(|(_, r)| r.score == best).map(|(i, _)| i).collect();

    let share = 1.0 / winners.len() as f64;
    for &i in &winners {
        equities[i] = share;
    }
    equities
}

fn enumerate_equity(
    players: &[(u8, &Vec<Card>)],
    board: &[Card],
    deck: &[Card],
    cards_needed: usize,
) -> Vec<f64> {
    let n = players.len();
    let mut wins = vec![0.0_f64; n];
    let mut total = 0u64;

    let dk = deck.len();
    let mut full_board = Vec::with_capacity(5);

    if cards_needed == 1 {
        for card in deck {
            full_board.clear();
            full_board.extend_from_slice(board);
            full_board.push(*card);

            tally_result(players, &full_board, &mut wins);
            total += 1;
        }
    } else {
        for i in 0..dk {
            for j in (i + 1)..dk {
                full_board.clear();
                full_board.extend_from_slice(board);
                full_board.push(deck[i]);
                full_board.push(deck[j]);

                tally_result(players, &full_board, &mut wins);
                total += 1;
            }
        }
    }

    if total == 0 {
        return vec![1.0 / n as f64; n];
    }

    let t = total as f64;
    wins.iter().map(|w| w / t).collect()
}

fn monte_carlo_equity(
    players: &[(u8, &Vec<Card>)],
    board: &[Card],
    deck: &[Card],
    cards_needed: usize,
    samples: usize,
) -> Vec<f64> {
    let n = players.len();
    let mut wins = vec![0.0_f64; n];
    let dk = deck.len();
    let mut rng_state: u64 = 0xDEAD_BEEF_CAFE_1234;
    let mut full_board = Vec::with_capacity(5);

    for _ in 0..samples {
        full_board.clear();
        full_board.extend_from_slice(board);

        let mut used = [false; 52];
        for _ in 0..cards_needed {
            loop {
                rng_state = rng_state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let idx = ((rng_state >> 33) as usize) % dk;
                if !used[idx] {
                    used[idx] = true;
                    full_board.push(deck[idx]);
                    break;
                }
            }
        }

        tally_result(players, &full_board, &mut wins);
    }

    let t = samples as f64;
    wins.iter().map(|w| w / t).collect()
}

fn tally_result(players: &[(u8, &Vec<Card>)], board: &[Card], wins: &mut [f64]) {
    let mut best_score = 0u32;
    let mut scores = Vec::with_capacity(players.len());

    for (_, hole) in players {
        let mut combined = Vec::with_capacity(hole.len() + board.len());
        combined.extend_from_slice(hole);
        combined.extend_from_slice(board);
        let rank = evaluate(&combined);
        let s = rank.score;
        if s > best_score {
            best_score = s;
        }
        scores.push(s);
    }

    let mut winner_count = 0u32;
    for &s in &scores {
        if s == best_score {
            winner_count += 1;
        }
    }

    let share = 1.0 / f64::from(winner_count);
    for (i, &s) in scores.iter().enumerate() {
        if s == best_score {
            wins[i] += share;
        }
    }
}

fn pct(num: u32, den: u32) -> Option<f64> {
    if den == 0 { None } else { Some(f64::from(num) / f64::from(den) * 100.0) }
}

fn fmt_pct(num: u32, den: u32) -> String {
    match pct(num, den) {
        Some(v) => format!("{v:.1}%"),
        None => "-".to_string(),
    }
}

fn fmt_af(bets: u32, calls: u32) -> String {
    if calls == 0 {
        if bets == 0 { "-".to_string() } else { "inf".to_string() }
    } else {
        format!("{:.2}", f64::from(bets) / f64::from(calls))
    }
}

fn fmt_bb(v: f64) -> String {
    if v >= 0.0 { format!("+{v:.1}") } else { format!("{v:.1}") }
}

fn print_player_stats(s: &PlayerStats, rank: Option<usize>) {
    let bb_per_hand =
        if s.hands_at_table > 0 { s.net_bb / f64::from(s.hands_at_table) } else { 0.0 };

    if let Some(r) = rank {
        println!("{}. {} (ID: {})", r + 1, s.name, s.player_id);
    } else {
        println!("{} (ID: {})", s.name, s.player_id);
    }
    println!("   Hands: {}/{} (played/total)", s.hands_played, s.hands_at_table);
    println!("   P&L: {} BB ({} BB/hand)", fmt_bb(s.net_bb), fmt_bb(bb_per_hand));
    println!();

    println!(
        "   Preflop:  VPIP {}  PFR {}  3-Bet {}  Fold-to-3B {}",
        fmt_pct(s.vpip_hands, s.hands_played),
        fmt_pct(s.pfr_hands, s.hands_played),
        fmt_pct(s.three_bets, s.three_bet_opp),
        fmt_pct(s.fold_to_three_bets, s.fold_to_three_bet_opp),
    );
    println!(
        "             Open-raise {}  Limp {}  Cold-call {}",
        s.open_raises, s.limps, s.cold_calls
    );

    println!(
        "   Postflop: C-bet {}  Fold-to-CB {}  AF {}",
        fmt_pct(s.cbets, s.cbet_opp),
        fmt_pct(s.fold_to_cbets, s.fold_to_cbet_opp),
        fmt_af(s.postflop_bets, s.postflop_calls),
    );

    println!(
        "   Showdown: WTSD {}  W$SD {}  WWSF {}",
        fmt_pct(s.went_to_showdown, s.saw_flop),
        fmt_pct(s.won_at_showdown, s.went_to_showdown),
        fmt_pct(s.won_when_saw_flop, s.saw_flop),
    );

    if s.all_in_hands > 0 {
        let ev_adj = s.net_bb + s.all_in_ev_diff;
        let direction = if s.all_in_ev_diff >= 0.0 { "above" } else { "below" };
        println!(
            "   All-in EV: ran {:.0} BB {} EV (EV-adjusted: {} BB)",
            s.all_in_ev_diff.abs(),
            direction,
            fmt_bb(ev_adj),
        );
    }
    println!();

    println!("   Position VPIP/PFR:");
    let labels = ["EP", "MP", "CO", "BTN", "SB", "BB"];
    let mut pos_line = String::from("     ");
    for (i, label) in labels.iter().enumerate() {
        let vpip = fmt_pct(s.pos_vpip[i], s.pos_hands[i]);
        let pfr = fmt_pct(s.pos_pfr[i], s.pos_hands[i]);
        if i > 0 {
            pos_line.push_str("  ");
        }
        let _ = write!(pos_line, "{label:<3} {vpip}/{pfr}");
    }
    println!("{pos_line}");
    println!();
}

pub fn print_stats(stats: &[PlayerStats]) {
    println!("Player Stats (ranked by P&L)");
    println!("============================\n");

    for (rank, s) in stats.iter().enumerate() {
        print_player_stats(s, Some(rank));
    }
}

pub fn print_single_player_stats(stats: &[PlayerStats], name: &str) {
    let lower = name.to_ascii_lowercase();
    let found = stats.iter().find(|s| s.name.to_ascii_lowercase() == lower);
    match found {
        Some(s) => print_player_stats(s, None),
        None => eprintln!("Player '{name}' not found in stats"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_helpers::*;

    fn stats_for(data: &crate::parser::GameData, id: &str) -> PlayerStats {
        let all = compute_stats(data);
        all.into_iter().find(|s| s.player_id == id).unwrap()
    }

    // --- VPIP / PFR ---

    #[test]
    fn vpip_counts_call_and_raise() {
        // Hand 1: p1 raises, p2 calls, p3 folds
        let h1 = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .call(2, 3.0)
            .fold(3)
            .win(1, 6.5);

        // Hand 2: p1 folds, p2 folds, p3 wins
        let h2 = HandBuilder::new()
            .number(2)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(2)
            .sb(3, 0.5)
            .bb(1, 1.0)
            .fold(2)
            .fold(3)
            .win(1, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2]);

        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.hands_played, 2);
        assert_eq!(s1.vpip_hands, 1); // raised in hand 1
        assert_eq!(s1.pfr_hands, 1);

        let s2 = stats_for(&data, "p2");
        assert_eq!(s2.vpip_hands, 1); // called in hand 1
        assert_eq!(s2.pfr_hands, 0); // never raised

        let s3 = stats_for(&data, "p3");
        assert_eq!(s3.vpip_hands, 0); // folded both hands
    }

    #[test]
    fn dead_blind_not_vpip() {
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

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.vpip_hands, 0);
    }

    // --- 3-Bet ---

    #[test]
    fn three_bet_tracking() {
        // p1 opens, p2 3-bets, p1 folds
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .bet(2, 9.0)
            .fold(3)
            .fold(1)
            .win(2, 13.0);

        let data = parse_game_data(&b);
        let s2 = stats_for(&data, "p2");
        assert_eq!(s2.three_bets, 1);
        assert!(s2.three_bet_opp >= 1);

        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.fold_to_three_bets, 1);
        assert_eq!(s1.fold_to_three_bet_opp, 1);
    }

    // --- C-Bet ---

    #[test]
    fn cbet_tracking() {
        // p1 raises preflop, then bets flop (c-bet)
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
            .bet(1, 4.0) // c-bet
            .fold(2)
            .win(1, 10.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.cbets, 1);
        assert_eq!(s1.cbet_opp, 1);

        let s2 = stats_for(&data, "p2");
        assert_eq!(s2.fold_to_cbets, 1);
        assert_eq!(s2.fold_to_cbet_opp, 1);
    }

    // --- Aggression factor ---

    #[test]
    fn aggression_factor_postflop() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .flop(&["Ah", "Kd", "Qs"])
            .bet(1, 2.0) // postflop bet
            .call(2, 2.0) // postflop call
            .turn("Js")
            .bet(1, 4.0)
            .call(2, 4.0)
            .win(1, 14.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.postflop_bets, 2);
        assert_eq!(s1.postflop_calls, 0);
        // AF = 2/0 = inf

        let s2 = stats_for(&data, "p2");
        assert_eq!(s2.postflop_bets, 0);
        assert_eq!(s2.postflop_calls, 2);
    }

    // --- WTSD / W$SD ---

    #[test]
    fn wtsd_and_wsd() {
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
            .showdown()
            .show(1, &["9s", "8s"])
            .show(2, &["7s", "6s"])
            .win(1, 2.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.saw_flop, 1);
        assert_eq!(s1.went_to_showdown, 1);
        assert_eq!(s1.won_at_showdown, 1);

        let s2 = stats_for(&data, "p2");
        assert_eq!(s2.saw_flop, 1);
        assert_eq!(s2.went_to_showdown, 1);
        assert_eq!(s2.won_at_showdown, 0);
    }

    #[test]
    fn wwsf_tracking() {
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
            .bet(1, 5.0)
            .fold(2)
            .win(1, 11.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.saw_flop, 1);
        assert_eq!(s1.won_when_saw_flop, 1);

        let s2 = stats_for(&data, "p2");
        assert_eq!(s2.saw_flop, 1);
        assert_eq!(s2.won_when_saw_flop, 0);

        let s3 = stats_for(&data, "p3");
        assert_eq!(s3.saw_flop, 0);
        assert_eq!(s3.won_when_saw_flop, 0);
    }

    // --- Positional stats ---

    #[test]
    fn positional_vpip_pfr() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1) // p1=BTN, p2=SB, p3=BB
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0) // BTN raise
            .fold(2)
            .fold(3)
            .win(1, 4.5);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        // BTN = index 3
        assert_eq!(s1.pos_vpip[3], 1);
        assert_eq!(s1.pos_pfr[3], 1);
        assert_eq!(s1.pos_hands[3], 1);
        // Other positions should be 0
        assert_eq!(s1.pos_hands[0], 0);
    }

    // --- Net P&L ---

    #[test]
    fn net_pnl_in_bb() {
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

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        // won 2.0 - invested 1.0 = net +1.0, in BB = +1.0
        assert!((s1.net_bb - 1.0).abs() < 0.001);

        let s2 = stats_for(&data, "p2");
        assert!((s2.net_bb - (-1.0)).abs() < 0.001);
    }

    // --- Limp tracking ---

    #[test]
    fn limp_counted() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .call(1, 1.0) // limp
            .call(2, 1.0) // complete from SB, still a limp
            .check(3)
            .win(3, 3.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.limps, 1);
    }

    // --- Open raise ---

    #[test]
    fn open_raise_counted() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0) // open raise
            .fold(2)
            .fold(3)
            .win(1, 4.5);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "p1");
        assert_eq!(s1.open_raises, 1);
    }

    // --- fmt helpers ---

    #[test]
    fn fmt_pct_works() {
        assert_eq!(fmt_pct(1, 2), "50.0%");
        assert_eq!(fmt_pct(0, 0), "-");
        assert_eq!(fmt_pct(3, 3), "100.0%");
    }

    #[test]
    fn fmt_af_works() {
        assert_eq!(fmt_af(0, 0), "-");
        assert_eq!(fmt_af(5, 0), "inf");
        assert_eq!(fmt_af(6, 3), "2.00");
    }

    #[test]
    fn fmt_bb_works() {
        assert_eq!(fmt_bb(10.5), "+10.5");
        assert_eq!(fmt_bb(-3.2), "-3.2");
        assert_eq!(fmt_bb(0.0), "+0.0");
    }

    // --- print_stats doesn't panic ---

    #[test]
    fn print_stats_no_panic() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let data = parse_game_data(&b);
        let stats = compute_stats(&data);
        print_stats(&stats);
    }
}
