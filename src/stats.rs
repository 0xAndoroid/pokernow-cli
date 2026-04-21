use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use rayon::prelude::*;

use crate::card::Card;
use crate::ev::{self, EvConfig};
use crate::parser::{
    ActionType, GameData, Hand, Position, Street, StreetData, invested, net_profit, saw_street,
    went_to_showdown,
};

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
    pub net_chips: f64,

    pub pos_vpip: [u32; 5],
    pub pos_pfr: [u32; 5],
    pub pos_hands: [u32; 5],

    pub all_in_ev_diff: f64,
    pub all_in_ev_diff_chips: f64,
    pub all_in_hands: u32,
}

impl std::ops::AddAssign<&PlayerStats> for PlayerStats {
    fn add_assign(&mut self, rhs: &PlayerStats) {
        if self.player_id.is_empty() {
            self.player_id.clone_from(&rhs.player_id);
        }
        if self.name.is_empty() {
            self.name.clone_from(&rhs.name);
        }
        self.hands_played += rhs.hands_played;
        self.hands_at_table += rhs.hands_at_table;
        self.vpip_hands += rhs.vpip_hands;
        self.pfr_hands += rhs.pfr_hands;
        self.three_bet_opp += rhs.three_bet_opp;
        self.three_bets += rhs.three_bets;
        self.fold_to_three_bet_opp += rhs.fold_to_three_bet_opp;
        self.fold_to_three_bets += rhs.fold_to_three_bets;
        self.cold_call_opp += rhs.cold_call_opp;
        self.cold_calls += rhs.cold_calls;
        self.open_raises += rhs.open_raises;
        self.limps += rhs.limps;
        self.cbet_opp += rhs.cbet_opp;
        self.cbets += rhs.cbets;
        self.fold_to_cbet_opp += rhs.fold_to_cbet_opp;
        self.fold_to_cbets += rhs.fold_to_cbets;
        self.postflop_bets += rhs.postflop_bets;
        self.postflop_calls += rhs.postflop_calls;
        self.saw_flop += rhs.saw_flop;
        self.won_when_saw_flop += rhs.won_when_saw_flop;
        self.went_to_showdown += rhs.went_to_showdown;
        self.won_at_showdown += rhs.won_at_showdown;
        self.net_bb += rhs.net_bb;
        self.net_chips += rhs.net_chips;
        for i in 0..5 {
            self.pos_vpip[i] += rhs.pos_vpip[i];
            self.pos_pfr[i] += rhs.pos_pfr[i];
            self.pos_hands[i] += rhs.pos_hands[i];
        }
        self.all_in_ev_diff += rhs.all_in_ev_diff;
        self.all_in_ev_diff_chips += rhs.all_in_ev_diff_chips;
        self.all_in_hands += rhs.all_in_hands;
    }
}

fn pos_index(pos: Position, is_straddle: bool) -> usize {
    if is_straddle {
        return 4; // Blinds
    }
    match pos {
        Position::EP => 0,
        Position::MP => 1,
        Position::CO => 2,
        Position::BTN => 3,
        Position::SB | Position::BB => 4, // Blinds
    }
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

fn get_hole_cards(hand: &Hand, seat: u8) -> Option<&Vec<Card>> {
    hand.players.iter().find(|p| p.seat == seat).and_then(|p| p.hole_cards.as_ref())
}

pub struct StatsResult {
    pub total_hands: usize,
    pub players: Vec<PlayerStats>,
}

fn resolve_name(data: &GameData, p: &crate::parser::PlayerInHand) -> String {
    data.player_names.get(&p.id).cloned().unwrap_or_else(|| p.name.clone())
}

pub fn compute_stats(data: &GameData) -> StatsResult {
    compute_stats_with_ev_config(data, &EvConfig::default())
}

pub fn compute_stats_with_ev_config(data: &GameData, cfg: &EvConfig) -> StatsResult {
    let map: HashMap<String, PlayerStats> = data
        .hands
        .par_iter()
        .enumerate()
        .map(|(idx, h)| hand_stats(h, data, cfg, idx as u64))
        .reduce(HashMap::new, merge_maps);

    let mut players: Vec<PlayerStats> = map.into_values().collect();
    players.sort_unstable_by(|a, b| b.net_bb.total_cmp(&a.net_bb));
    StatsResult {
        total_hands: data.hands.len(),
        players,
    }
}

fn merge_maps(
    mut acc: HashMap<String, PlayerStats>,
    other: HashMap<String, PlayerStats>,
) -> HashMap<String, PlayerStats> {
    use std::collections::hash_map::Entry;
    for (name, stats) in other {
        match acc.entry(name) {
            Entry::Occupied(mut e) => {
                *e.get_mut() += &stats;
            }
            Entry::Vacant(e) => {
                e.insert(stats);
            }
        }
    }
    acc
}

fn hand_stats(
    hand: &Hand,
    data: &GameData,
    cfg: &EvConfig,
    hand_idx: u64,
) -> HashMap<String, PlayerStats> {
    let mut map: HashMap<String, PlayerStats> = HashMap::new();
    let seat_to_name: HashMap<u8, String> =
        hand.players.iter().map(|p| (p.seat, resolve_name(data, p))).collect();

    for p in &hand.players {
        let name = &seat_to_name[&p.seat];
        let stats = map.entry(name.clone()).or_insert_with(|| PlayerStats {
            player_id: p.id.clone(),
            name: name.clone(),
            ..PlayerStats::default()
        });

        stats.hands_at_table += 1;
        let profit = net_profit(hand, p.seat);
        stats.net_chips += profit;
        if hand.effective_bb > 0.0 {
            stats.net_bb += profit / hand.effective_bb;
        }

        if !hand.bomb_pot {
            stats.hands_played += 1;
            let is_str = hand.straddle_seat == Some(p.seat);
            stats.pos_hands[pos_index(p.position, is_str)] += 1;
        }
    }

    process_preflop(hand, &seat_to_name, &mut map);
    process_postflop(hand, &seat_to_name, &mut map);
    process_showdown(hand, &seat_to_name, &mut map);
    process_all_in_ev(hand, &seat_to_name, &mut map, cfg, hand_idx);

    map
}

fn process_preflop(
    hand: &Hand,
    seat_to_name: &HashMap<u8, String>,
    map: &mut HashMap<String, PlayerStats>,
) {
    if hand.bomb_pot {
        return;
    }

    let preflop = match hand.streets.first() {
        Some(sd) if sd.street == Street::Preflop => sd,
        _ => return,
    };

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
        let name = match seat_to_name.get(&seat) {
            Some(n) => n.as_str(),
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
                    && let Some(stats) = map.get_mut(name)
                {
                    stats.fold_to_three_bet_opp += 1;
                    stats.fold_to_three_bets += 1;
                }
                folded.insert(seat);
            }
            ActionType::Call => {
                vpip_seats.insert(seat);
                if raise_count == 0 {
                    if let Some(stats) = map.get_mut(name) {
                        stats.limps += 1;
                    }
                } else if raise_count >= 1 && !has_voluntarily_acted.contains(&seat) {
                    let is_bb = seat_to_pos.get(&seat) == Some(&Position::BB);
                    if raise_count == 1
                        && !is_bb
                        && let Some(stats) = map.get_mut(name)
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
                    && let Some(stats) = map.get_mut(name)
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
                    if let Some(stats) = map.get_mut(name) {
                        stats.open_raises += 1;
                    }
                } else if raise_count == 2 {
                    three_bettor = Some(seat);
                    if open_raiser != Some(seat) {
                        faced_open_raise.insert(seat);
                    }
                    if let Some(stats) = map.get_mut(name) {
                        stats.three_bets += 1;
                    }
                }

                // 4-bet by the open raiser = they faced the 3-bet (opportunity)
                // but chose to re-raise instead of folding or calling.
                if raise_count == 3
                    && three_bettor.is_some()
                    && Some(seat) == open_raiser
                    && let Some(stats) = map.get_mut(name)
                {
                    stats.fold_to_three_bet_opp += 1;
                }

                if raise_count == 2
                    && !has_voluntarily_acted.contains(&seat)
                    && seat_to_pos.get(&seat) != Some(&Position::BB)
                    && let Some(stats) = map.get_mut(name)
                {
                    stats.cold_call_opp += 1;
                }

                has_voluntarily_acted.insert(seat);
            }
            _ => {}
        }
    }

    for &seat in &faced_open_raise {
        let Some(name) = seat_to_name.get(&seat) else { continue };
        if let Some(stats) = map.get_mut(name.as_str()) {
            stats.three_bet_opp += 1;
        }
    }
    if let Some(tb) = three_bettor
        && !faced_open_raise.contains(&tb)
    {
        let Some(name) = seat_to_name.get(&tb) else { return };
        if let Some(stats) = map.get_mut(name.as_str()) {
            stats.three_bet_opp += 1;
        }
    }

    for &seat in &vpip_seats {
        let Some(name) = seat_to_name.get(&seat) else { continue };
        if let Some(stats) = map.get_mut(name.as_str()) {
            stats.vpip_hands += 1;
            if let Some(&pos) = seat_to_pos.get(&seat) {
                let is_str = hand.straddle_seat == Some(seat);
                stats.pos_vpip[pos_index(pos, is_str)] += 1;
            }
        }
    }
    for &seat in &pfr_seats {
        let Some(name) = seat_to_name.get(&seat) else { continue };
        if let Some(stats) = map.get_mut(name.as_str()) {
            stats.pfr_hands += 1;
            if let Some(&pos) = seat_to_pos.get(&seat) {
                let is_str = hand.straddle_seat == Some(seat);
                stats.pos_pfr[pos_index(pos, is_str)] += 1;
            }
        }
    }
}

fn process_postflop(
    hand: &Hand,
    seat_to_name: &HashMap<u8, String>,
    map: &mut HashMap<String, PlayerStats>,
) {
    let preflop = match hand.streets.first() {
        Some(sd) if sd.street == Street::Preflop => sd,
        _ => return,
    };

    let pf_aggressor = if hand.bomb_pot { None } else { preflop_aggressor(preflop) };

    let winner_seats: HashSet<u8> = hand.winners.iter().map(|w| w.seat).collect();

    for p in &hand.players {
        if saw_street(hand, p.seat, Street::Flop)
            && hand.streets.iter().any(|sd| sd.street >= Street::Flop)
            && let Some(name) = seat_to_name.get(&p.seat)
            && let Some(stats) = map.get_mut(name.as_str())
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
            let name = match seat_to_name.get(&a.seat) {
                Some(n) => n.as_str(),
                None => continue,
            };

            match a.kind {
                ActionType::Bet => {
                    if let Some(stats) = map.get_mut(name) {
                        stats.postflop_bets += 1;
                    }
                    if first_bettor.is_none() {
                        first_bettor = Some(a.seat);

                        if sd.street == Street::Flop
                            && Some(a.seat) == pf_aggressor
                            && let Some(stats) = map.get_mut(name)
                        {
                            stats.cbets += 1;
                        }
                    }
                }
                ActionType::Call => {
                    if let Some(stats) = map.get_mut(name) {
                        stats.postflop_calls += 1;
                    }
                }
                ActionType::Fold => {
                    if sd.street == Street::Flop
                        && first_bettor.is_some()
                        && first_bettor == pf_aggressor
                        && let Some(stats) = map.get_mut(name)
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
                && let Some(name) = seat_to_name.get(&agg)
                && let Some(stats) = map.get_mut(name.as_str())
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
                            if let Some(name) = seat_to_name.get(&a.seat)
                                && let Some(stats) = map.get_mut(name.as_str())
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

fn process_showdown(
    hand: &Hand,
    seat_to_name: &HashMap<u8, String>,
    map: &mut HashMap<String, PlayerStats>,
) {
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
            && let Some(name) = seat_to_name.get(&p.seat)
            && let Some(stats) = map.get_mut(name.as_str())
        {
            stats.went_to_showdown += 1;
            if winner_seats.contains(&p.seat) {
                stats.won_at_showdown += 1;
            }
        }
    }
}

fn process_all_in_ev(
    hand: &Hand,
    seat_to_name: &HashMap<u8, String>,
    map: &mut HashMap<String, PlayerStats>,
    cfg: &EvConfig,
    hand_idx: u64,
) {
    if !hand.real_showdown || hand.effective_bb <= 0.0 {
        return;
    }

    let mut last_ai_street: Option<Street> = None;
    for sd in &hand.streets {
        if sd.actions.iter().any(|a| a.all_in) {
            last_ai_street = Some(sd.street);
        }
    }
    let Some(ai_street) = last_ai_street else { return };

    let has_action_after = hand.streets.iter().any(|sd| {
        sd.street > ai_street
            && sd.actions.iter().any(|a| matches!(a.kind, ActionType::Bet | ActionType::Call))
    });
    if has_action_after {
        return;
    }

    let folded_seats: HashSet<u8> = hand
        .streets
        .iter()
        .flat_map(|sd| sd.actions.iter())
        .filter(|a| a.kind == ActionType::Fold)
        .map(|a| a.seat)
        .collect();

    let mut known: Vec<(u8, &Vec<Card>)> = Vec::new();
    for p in &hand.players {
        if folded_seats.contains(&p.seat) {
            continue;
        }
        if let Some(cards) = get_hole_cards(hand, p.seat)
            && cards.len() == 2
        {
            known.push((p.seat, cards));
        }
    }

    if known.len() < 2 {
        return;
    }

    let board = board_at_street(hand, ai_street);

    let folded_investments: Vec<f64> = hand
        .players
        .iter()
        .filter(|p| folded_seats.contains(&p.seat))
        .map(|p| invested(hand, p.seat))
        .collect();

    let mut investments: Vec<f64> = known.iter().map(|&(seat, _)| invested(hand, seat)).collect();
    for &(seat, _) in &known {
        if let Some(&ret) = hand.uncalled_returns.get(&seat) {
            let idx = known.iter().position(|&(s, _)| s == seat).unwrap();
            investments[idx] -= ret;
        }
    }

    let mut sorted_inv: Vec<f64> = investments.clone();
    sorted_inv.sort_unstable_by(f64::total_cmp);
    sorted_inv.dedup();

    let n = known.len();
    let mut ev_expected = vec![0.0_f64; n];

    // Build pot slices. Each slice is contested by eligible players only.
    // Equity must be computed per-slice because the eligible player subset
    // changes, and global equity renormalization is incorrect (a player who
    // loses to a non-eligible player may still win the side pot).
    let mut prev_threshold = 0.0_f64;
    for (slice_idx, &threshold) in sorted_inv.iter().enumerate() {
        let marginal = threshold - prev_threshold;
        if marginal <= 0.0 {
            continue;
        }

        let eligible: Vec<usize> = (0..n).filter(|&i| investments[i] >= threshold).collect();
        let slice_size = marginal * eligible.len() as f64;

        // Dead money from folded players: each contributes
        // min(their_investment, threshold) - min(their_investment, prev_threshold)
        let dead_in_slice: f64 = folded_investments
            .iter()
            .map(|&fi| fi.min(threshold) - fi.min(prev_threshold))
            .filter(|&v| v > 0.0)
            .sum();
        let total_slice = slice_size + dead_in_slice;

        if eligible.len() == 1 {
            ev_expected[eligible[0]] += total_slice;
        } else {
            let slice_players: Vec<(u8, &Vec<Card>)> = eligible.iter().map(|&i| known[i]).collect();
            let seed_salt = ev::mix_seed(hand_idx, slice_idx as u64, 0);
            let slice_eq = ev::calculate_multi_equity(&slice_players, &board, hand, cfg, seed_salt);

            for (j, &i) in eligible.iter().enumerate() {
                ev_expected[i] += slice_eq[j] * total_slice;
            }
        }

        prev_threshold = threshold;
    }

    for (i, &(seat, _)) in known.iter().enumerate() {
        let actual_from_pot: f64 =
            hand.winners.iter().filter(|w| w.seat == seat).map(|w| w.amount).sum();

        let ev_diff_chips = ev_expected[i] - actual_from_pot;
        let ev_diff = ev_diff_chips / hand.effective_bb;

        if let Some(name) = seat_to_name.get(&seat)
            && let Some(stats) = map.get_mut(name.as_str())
        {
            stats.all_in_ev_diff += ev_diff;
            stats.all_in_ev_diff_chips += ev_diff_chips;
            stats.all_in_hands += 1;
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

fn fmt_signed(v: f64) -> String {
    if v >= 0.0 { format!("+{v:.1}") } else { format!("{v:.1}") }
}

fn print_player_stats(s: &PlayerStats, rank: Option<usize>, total_hands: usize, use_chips: bool) {
    if let Some(r) = rank {
        println!("{}. {} (ID: {})", r + 1, s.name, s.player_id);
    } else {
        println!("{} (ID: {})", s.name, s.player_id);
    }
    println!("   Hands: {}/{} (dealt/total)", s.hands_at_table, total_hands);

    if use_chips {
        let per_hand =
            if s.hands_at_table > 0 { s.net_chips / f64::from(s.hands_at_table) } else { 0.0 };
        println!("   P&L: {} ({}/hand)", fmt_signed(s.net_chips), fmt_signed(per_hand),);
    } else {
        let per_hand =
            if s.hands_at_table > 0 { s.net_bb / f64::from(s.hands_at_table) } else { 0.0 };
        println!("   P&L: {} BB ({} BB/hand)", fmt_bb(s.net_bb), fmt_bb(per_hand));
    }
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
        if use_chips {
            let ev_adj = s.net_chips + s.all_in_ev_diff_chips;
            let direction = if s.all_in_ev_diff_chips >= 0.0 { "below" } else { "above" };
            println!(
                "   All-in EV: ran {:.0} {} EV (EV-adjusted: {})",
                s.all_in_ev_diff_chips.abs(),
                direction,
                fmt_signed(ev_adj),
            );
        } else {
            let ev_adj = s.net_bb + s.all_in_ev_diff;
            let direction = if s.all_in_ev_diff >= 0.0 { "below" } else { "above" };
            println!(
                "   All-in EV: ran {:.0} BB {} EV (EV-adjusted: {} BB)",
                s.all_in_ev_diff.abs(),
                direction,
                fmt_bb(ev_adj),
            );
        }
    }
    println!();

    println!("   Position VPIP/PFR:");
    let labels = ["EP", "MP", "CO", "BTN", "Blinds"];
    let mut pos_line = String::from("     ");
    for (i, label) in labels.iter().enumerate() {
        let vpip = fmt_pct(s.pos_vpip[i], s.pos_hands[i]);
        let pfr = fmt_pct(s.pos_pfr[i], s.pos_hands[i]);
        if i > 0 {
            pos_line.push_str("  ");
        }
        let _ = write!(pos_line, "{label:<6} {vpip}/{pfr}");
    }
    println!("{pos_line}");
    println!();
}

pub fn print_stats(result: &StatsResult, use_chips: bool) {
    println!("Player Stats (ranked by P&L)");
    println!("============================\n");

    for (rank, s) in result.players.iter().enumerate() {
        print_player_stats(s, Some(rank), result.total_hands, use_chips);
    }
}

pub fn print_single_player_stats(result: &StatsResult, name: &str, use_chips: bool) {
    let lower = name.to_ascii_lowercase();
    let found = result.players.iter().find(|s| s.name.to_ascii_lowercase() == lower);
    match found {
        Some(s) => print_player_stats(s, None, result.total_hands, use_chips),
        None => eprintln!("Player '{name}' not found in stats"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_helpers::*;

    fn stats_for(data: &crate::parser::GameData, name: &str) -> PlayerStats {
        // Tests must be reproducible: pin the MC seed so convergence
        // checks and cross-hand equality assertions are stable.
        let cfg = EvConfig::deterministic(0x5EED_1234_5678_9ABC);
        let result = compute_stats_with_ev_config(data, &cfg);
        result.players.into_iter().find(|s| s.name == name).unwrap()
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

        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.hands_played, 2);
        assert_eq!(s1.vpip_hands, 1); // raised in hand 1
        assert_eq!(s1.pfr_hands, 1);

        let s2 = stats_for(&data, "Bob");
        assert_eq!(s2.vpip_hands, 1); // called in hand 1
        assert_eq!(s2.pfr_hands, 0); // never raised

        let s3 = stats_for(&data, "Charlie");
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
        let s1 = stats_for(&data, "Alice");
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
        let s2 = stats_for(&data, "Bob");
        assert_eq!(s2.three_bets, 1);
        assert!(s2.three_bet_opp >= 1);

        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.fold_to_three_bets, 1);
        assert_eq!(s1.fold_to_three_bet_opp, 1);
    }

    // Open → 3-bet → 4-bet: opener faced the 3-bet (fold_to_three_bet_opp)
    // but didn't fold (fold_to_three_bets stays 0).
    #[test]
    fn fold_to_three_bet_opp_counted_on_4bet() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0) // open raise
            .bet(2, 9.0) // 3-bet
            .fold(3)
            .bet(1, 27.0) // 4-bet (open raiser re-raises)
            .fold(2)
            .win(1, 36.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        assert_eq!(
            s1.fold_to_three_bet_opp, 1,
            "open raiser faced the 3-bet (4-bet counts as an opportunity)"
        );
        assert_eq!(s1.fold_to_three_bets, 0, "open raiser did not fold to the 3-bet");
    }

    // Open → 3-bet → call: fold_to_three_bet_opp = 1, fold_to_three_bets = 0.
    #[test]
    fn fold_to_three_bet_call_counted_as_opp() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0) // open
            .bet(2, 9.0) // 3-bet
            .fold(3)
            .call(1, 9.0) // call the 3-bet
            .flop(&["Ah", "Kd", "Qs"])
            .check(1)
            .check(2)
            .win(1, 19.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.fold_to_three_bet_opp, 1);
        assert_eq!(s1.fold_to_three_bets, 0);
    }

    #[test]
    fn fold_to_three_bet_via_action_marker() {
        // PokerNow represents preflop folds as ActionMarker (type 11).
        // Open raiser folds to 3-bet via ActionMarker.
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0) // open raise
            .action_marker(2) // SB folds (ActionMarker)
            .bet(3, 9.0) // 3-bet
            .action_marker(1) // opener folds to 3-bet (ActionMarker)
            .win(3, 12.5);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.fold_to_three_bet_opp, 1, "opener faced 3-bet");
        assert_eq!(s1.fold_to_three_bets, 1, "opener folded to 3-bet");
        assert_eq!(s1.pfr_hands, 1, "opener raised preflop");
    }

    #[test]
    fn fold_to_three_bet_with_spurious_fold_and_action_marker() {
        // Opener has a spurious type-0 fold, then opens, then folds to 3-bet via ActionMarker
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(1) // spurious type-0 fold
            .bet(1, 3.0) // open raise (proves fold was spurious)
            .action_marker(2) // SB folds
            .bet(3, 9.0) // 3-bet
            .action_marker(1) // opener folds to 3-bet
            .win(3, 12.5);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.fold_to_three_bet_opp, 1);
        assert_eq!(s1.fold_to_three_bets, 1);
    }

    #[test]
    fn limper_not_counted_for_fold_to_three_bet() {
        // Limper faces raise then 3-bet → NOT a fold-to-3B opportunity for limper
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .player("p4", 4, "Dave", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .call(1, 1.0) // p1 limps
            .bet(4, 3.0) // p4 raises (open raiser)
            .action_marker(2) // SB folds
            .bet(3, 9.0) // BB 3-bets
            .action_marker(1) // limper folds
            .action_marker(4) // opener folds to 3-bet
            .win(3, 13.5);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.fold_to_three_bet_opp, 0, "limper should not get fold-to-3B opp");
        assert_eq!(s1.fold_to_three_bets, 0);

        let s4 = stats_for(&data, "Dave");
        assert_eq!(s4.fold_to_three_bet_opp, 1, "open raiser gets the opp");
        assert_eq!(s4.fold_to_three_bets, 1, "open raiser folded");
    }

    #[test]
    fn cold_caller_not_counted_for_fold_to_three_bet() {
        // Cold-caller faces 3-bet → NOT a fold-to-3B opportunity
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .player("p4", 4, "Dave", 100.0)
            .dealer(1)
            .sb(3, 0.5)
            .bb(4, 1.0)
            .bet(1, 3.0) // p1 opens
            .call(2, 3.0) // p2 cold-calls
            .action_marker(3) // SB folds
            .bet(4, 12.0) // BB 3-bets (squeeze)
            .action_marker(1) // opener folds
            .action_marker(2) // cold-caller folds
            .win(4, 18.5);

        let data = parse_game_data(&b);
        let s2 = stats_for(&data, "Bob");
        assert_eq!(s2.fold_to_three_bet_opp, 0, "cold-caller has no fold-to-3B opp");

        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.fold_to_three_bet_opp, 1);
        assert_eq!(s1.fold_to_three_bets, 1);
    }

    #[test]
    fn fold_to_cbet_via_action_marker() {
        // Player folds to c-bet via ActionMarker on flop
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .action_marker(2)
            .call(3, 3.0)
            .flop(&["Ah", "Kd", "Qs"])
            .bet(1, 4.0) // c-bet
            .action_marker(3) // folds to c-bet via ActionMarker
            .win(1, 10.0);

        let data = parse_game_data(&b);
        let s3 = stats_for(&data, "Charlie");
        assert_eq!(s3.fold_to_cbet_opp, 1, "BB faced c-bet");
        assert_eq!(s3.fold_to_cbets, 1, "BB folded to c-bet");
    }

    #[test]
    fn three_bet_opp_includes_action_marker_folds() {
        // Players who fold to open raise via ActionMarker should get 3-bet opportunity
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .player("p4", 4, "Dave", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0) // open
            .action_marker(4) // folds to open (had 3-bet opportunity)
            .action_marker(2) // SB folds to open
            .call(3, 3.0) // BB calls
            .flop(&["Ah", "Kd", "Qs"])
            .check(1)
            .check(3)
            .win(1, 6.5);

        let data = parse_game_data(&b);
        let s4 = stats_for(&data, "Dave");
        assert_eq!(s4.three_bet_opp, 1, "folding to open = had 3-bet opportunity");
        assert_eq!(s4.three_bets, 0);

        let s2 = stats_for(&data, "Bob");
        assert_eq!(s2.three_bet_opp, 1, "SB folding to open = had 3-bet opportunity");
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
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.cbets, 1);
        assert_eq!(s1.cbet_opp, 1);

        let s2 = stats_for(&data, "Bob");
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
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.postflop_bets, 2);
        assert_eq!(s1.postflop_calls, 0);
        // AF = 2/0 = inf

        let s2 = stats_for(&data, "Bob");
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
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.saw_flop, 1);
        assert_eq!(s1.went_to_showdown, 1);
        assert_eq!(s1.won_at_showdown, 1);

        let s2 = stats_for(&data, "Bob");
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
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.saw_flop, 1);
        assert_eq!(s1.won_when_saw_flop, 1);

        let s2 = stats_for(&data, "Bob");
        assert_eq!(s2.saw_flop, 1);
        assert_eq!(s2.won_when_saw_flop, 0);

        let s3 = stats_for(&data, "Charlie");
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
        let s1 = stats_for(&data, "Alice");
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
        let s1 = stats_for(&data, "Alice");
        // won 2.0 - invested 1.0 = net +1.0, in BB = +1.0
        assert!((s1.net_bb - 1.0).abs() < 0.001);

        let s2 = stats_for(&data, "Bob");
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
        let s1 = stats_for(&data, "Alice");
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
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.open_raises, 1);
    }

    // --- All-in EV ---

    fn preflop_allin_hand(bb: f64, stack: f64, p1_wins: bool) -> HandBuilder {
        // Both players go all-in preflop. p1=AA, p2=KK. Winner set by p1_wins.
        // Same hole cards and empty board → Monte Carlo equity is reproducible.
        let winner = if p1_wins { 1 } else { 2 };
        HandBuilder::new()
            .blinds(bb / 2.0, bb)
            .player_with_hand("p1", 1, "Alice", stack, &["Ah", "Ad"])
            .player_with_hand("p2", 2, "Bob", stack, &["Kh", "Kd"])
            .dealer(1)
            .sb(1, bb / 2.0)
            .bb(2, bb)
            .bet_all_in(1, stack)
            .call_all_in(2, stack)
            .flop(&["2c", "3d", "4s"])
            .turn("5h")
            .river("9c")
            .showdown()
            .show(1, &["Ah", "Ad"])
            .show(2, &["Kh", "Kd"])
            .win(winner, stack * 2.0)
    }

    // Both players all-in preflop: both must be tracked and ev_diffs sum to zero.
    #[test]
    fn all_in_ev_both_all_in_equal_stacks() {
        let b = preflop_allin_hand(1.0, 100.0, true);
        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        let s2 = stats_for(&data, "Bob");

        assert_eq!(s1.all_in_hands, 1);
        assert_eq!(s2.all_in_hands, 1);
        // Sum of ev_diffs is always 0: equity_p1 + equity_p2 = 1, actual_p1 + actual_p2 = pot.
        assert!((s1.all_in_ev_diff + s2.all_in_ev_diff).abs() < 0.01);
    }

    // Covering player: p1 raises preflop (NOT all-in), p2 calls all-in.
    // p1 never has all_in=true — this is the primary bug scenario.
    #[test]
    fn all_in_ev_covering_player_counted() {
        // p1 (200 chips) raises to 100 preflop — NOT all-in (100 chips remain).
        // p2 (100 chips) calls all-in for 100.
        let b = HandBuilder::new()
            .blinds(0.5, 1.0)
            .player_with_hand("p1", 1, "Alice", 200.0, &["Ah", "Ad"])
            .player_with_hand("p2", 2, "Bob", 100.0, &["Kh", "Kd"])
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet(1, 100.0) // NOT all-in: p1 has 100 chips remaining
            .call_all_in(2, 100.0)
            .flop(&["2c", "3d", "4s"])
            .turn("5h")
            .river("9c")
            .showdown()
            .show(1, &["Ah", "Ad"])
            .show(2, &["Kh", "Kd"])
            .win(1, 200.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        let s2 = stats_for(&data, "Bob");

        // Primary bug: covering player must be tracked even without all_in=true.
        assert_eq!(s1.all_in_hands, 1, "covering player must be tracked");
        assert_eq!(s2.all_in_hands, 1);
        // Both tracked → ev_diffs sum to 0.
        assert!((s1.all_in_ev_diff + s2.all_in_ev_diff).abs() < 0.01);
    }

    // When action continues after the all-in (side pot), EV should NOT be tracked.
    #[test]
    fn all_in_ev_skipped_when_action_continues_after() {
        // p1 all-in preflop, p2 and p3 both have chips left → postflop action possible.
        let b = HandBuilder::new()
            .blinds(0.5, 1.0)
            .player_with_hand("p1", 1, "Alice", 50.0, &["Ah", "Ad"])
            .player_with_hand("p2", 2, "Bob", 200.0, &["Kh", "Kd"])
            .player_with_hand("p3", 3, "Carol", 200.0, &["Qh", "Qd"])
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet_all_in(1, 50.0)
            .call(2, 50.0)
            .call(3, 50.0)
            .flop(&["2c", "3d", "4s"])
            // p2 and p3 still have chips → action continues after the all-in
            .bet(2, 10.0)
            .call(3, 10.0)
            .turn("5h")
            .check(2)
            .check(3)
            .river("9c")
            .check(2)
            .check(3)
            .showdown()
            .show(1, &["Ah", "Ad"])
            .show(2, &["Kh", "Kd"])
            .show(3, &["Qh", "Qd"])
            .win(1, 150.0)
            .win(2, 20.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        assert_eq!(s1.all_in_hands, 0, "EV must not be tracked when action continues");
    }

    // Uncalled return must not inflate the contested pot.
    // Verify both players are tracked and ev_diffs sum to zero.
    #[test]
    fn all_in_ev_uncalled_return_excluded() {
        // p1 (200 chips) goes all-in preflop for 200. p2 (100 chips) calls all-in for 100.
        // 100 chips returned to p1 uncalled. Contested pot = 200.
        let b = HandBuilder::new()
            .blinds(0.5, 1.0)
            .player_with_hand("p1", 1, "Alice", 200.0, &["Ah", "Ad"])
            .player_with_hand("p2", 2, "Bob", 100.0, &["Kh", "Kd"])
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet_all_in(1, 200.0)
            .call_all_in(2, 100.0)
            .uncalled_return(1, 100.0)
            .flop(&["2c", "3d", "4s"])
            .turn("5h")
            .river("9c")
            .showdown()
            .show(1, &["Ah", "Ad"])
            .show(2, &["Kh", "Kd"])
            .win(1, 200.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        let s2 = stats_for(&data, "Bob");

        assert_eq!(s1.all_in_hands, 1);
        assert_eq!(s2.all_in_hands, 1);
        assert!((s1.all_in_ev_diff + s2.all_in_ev_diff).abs() < 0.01);
    }

    // ev_diff must be in BBs so it's on the same scale as net_bb.
    // Identical hand at BB=1 (stack=100) vs BB=2 (stack=200): same proportional position,
    // same hole cards, same Monte Carlo seed → same equity → ev_diff must be equal.
    // If ev_diff were in chip units (not divided by BB), the BB=2 value would be 2×.
    #[test]
    fn all_in_ev_diff_in_bb_units() {
        let b1 = preflop_allin_hand(1.0, 100.0, true);
        let b2 = preflop_allin_hand(2.0, 200.0, true);

        let s1 = stats_for(&parse_game_data(&b1), "Alice");
        let s2 = stats_for(&parse_game_data(&b2), "Alice");

        assert!(
            (s1.all_in_ev_diff - s2.all_in_ev_diff).abs() < 0.01,
            "ev_diff(BB=1)={} ev_diff(BB=2)={} — must be equal when stacks are proportional",
            s1.all_in_ev_diff,
            s2.all_in_ev_diff
        );
    }

    // 3-way all-in with unequal stacks. Short stack can only win main pot.
    // Equity must be computed per pot slice with per-slice eligible players.
    #[test]
    fn all_in_ev_multiway_side_pot() {
        // p1 (50 chips), p2 (200 chips), p3 (200 chips).
        // Main pot = 50*3 = 150 (all 3). Side pot = 150*2 = 300 (p2+p3).
        // p1 has AA, p2 has KK, p3 has QQ. p1 wins main, p2 wins side.
        let b = HandBuilder::new()
            .blinds(0.5, 1.0)
            .player_with_hand("p1", 1, "Alice", 50.0, &["Ah", "Ad"])
            .player_with_hand("p2", 2, "Bob", 200.0, &["Kh", "Kd"])
            .player_with_hand("p3", 3, "Carol", 200.0, &["Qh", "Qd"])
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet_all_in(1, 50.0)
            .call(2, 50.0)
            .bet_all_in(3, 200.0)
            .call_all_in(2, 200.0)
            .flop(&["2c", "3d", "4s"])
            .turn("5h")
            .river("9c")
            .showdown()
            .show(1, &["Ah", "Ad"])
            .show(2, &["Kh", "Kd"])
            .show(3, &["Qh", "Qd"])
            .win(1, 150.0)
            .win(2, 300.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        let s2 = stats_for(&data, "Bob");
        let s3 = stats_for(&data, "Carol");

        assert_eq!(s1.all_in_hands, 1);
        assert_eq!(s2.all_in_hands, 1);
        assert_eq!(s3.all_in_hands, 1);

        // Conservation: EV diffs must sum to zero across all players.
        let total_ev = s1.all_in_ev_diff + s2.all_in_ev_diff + s3.all_in_ev_diff;
        assert!(
            total_ev.abs() < 1.0,
            "ev_diffs must sum to ~0, got {total_ev:.2} (p1={:.2}, p2={:.2}, p3={:.2})",
            s1.all_in_ev_diff,
            s2.all_in_ev_diff,
            s3.all_in_ev_diff
        );

        // Short stack won the main pot. AA has ~67% equity vs KK+QQ preflop.
        // p1's EV < main pot (150) because equity < 100%. Since p1 actually
        // received 150 (more than EV), ev_diff must be < 0 (ran above EV).
        assert!(
            s1.all_in_ev_diff < 0.0,
            "short stack AA won max; ev_diff should be < 0 (ran above EV), got {:.2}",
            s1.all_in_ev_diff
        );
    }

    // 2-player all-in where one player folds: EV should still track for the 2 remaining.
    #[test]
    fn all_in_ev_with_folded_player_dead_money() {
        // p3 raises then folds to a 3-bet shove. p1 and p2 go to showdown.
        // p3's money is dead money in the pot.
        let b = HandBuilder::new()
            .blinds(0.5, 1.0)
            .player_with_hand("p1", 1, "Alice", 100.0, &["Ah", "Ad"])
            .player_with_hand("p2", 2, "Bob", 100.0, &["Kh", "Kd"])
            .player("p3", 3, "Carol", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .fold(2)
            .bet(3, 10.0)
            .bet_all_in(1, 100.0)
            .call_all_in(2, 100.0)
            .fold(3)
            .flop(&["2c", "3d", "4s"])
            .turn("5h")
            .river("9c")
            .showdown()
            .show(1, &["Ah", "Ad"])
            .show(2, &["Kh", "Kd"])
            .win(1, 210.0);

        let data = parse_game_data(&b);
        let s1 = stats_for(&data, "Alice");
        let s2 = stats_for(&data, "Bob");

        assert_eq!(s1.all_in_hands, 1);
        assert_eq!(s2.all_in_hands, 1);

        let total_ev = s1.all_in_ev_diff + s2.all_in_ev_diff;
        assert!(
            total_ev.abs() < 0.1,
            "ev_diffs must sum to ~0 for the 2 contesting players, got {total_ev:.2}"
        );
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
        let result = compute_stats(&data);
        print_stats(&result, false);
    }

    #[test]
    fn total_hands_reflects_dataset_size() {
        let h1 = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .fold(2)
            .fold(3)
            .win(1, 4.5);

        // p3 sits out hand 2 (not in players list)
        let h2 = HandBuilder::new()
            .number(2)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(1, 1.0)
            .fold(2)
            .win(1, 1.5);

        // p1 and p3 sit out hand 3
        let h3 = HandBuilder::new()
            .number(3)
            .player("p2", 2, "Bob", 100.0)
            .player("p4", 4, "Dave", 100.0)
            .dealer(2)
            .sb(4, 0.5)
            .bb(2, 1.0)
            .fold(4)
            .win(2, 1.5);

        let data = parse_multi_game_data(&[&h1, &h2, &h3]);
        let result = compute_stats(&data);

        assert_eq!(result.total_hands, 3);

        let alice = result.players.iter().find(|s| s.name == "Alice").unwrap();
        assert_eq!(alice.hands_at_table, 2); // dealt into hands 1,2
        let bob = result.players.iter().find(|s| s.name == "Bob").unwrap();
        assert_eq!(bob.hands_at_table, 3); // dealt into all 3
        let charlie = result.players.iter().find(|s| s.name == "Charlie").unwrap();
        assert_eq!(charlie.hands_at_table, 1); // dealt into hand 1 only
        let dave = result.players.iter().find(|s| s.name == "Dave").unwrap();
        assert_eq!(dave.hands_at_table, 1); // dealt into hand 3 only
    }

    #[test]
    fn same_name_different_ids_merged() {
        let h1 = HandBuilder::new()
            .number(1)
            .player("id_v1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .bet(1, 3.0)
            .call(2, 3.0)
            .fold(3)
            .win(1, 6.5);

        let h2 = HandBuilder::new()
            .number(2)
            .player("id_v2", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(2)
            .sb(3, 0.5)
            .bb(1, 1.0)
            .bet(2, 3.0)
            .call(1, 3.0)
            .fold(3)
            .win(2, 6.5);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let result = compute_stats(&data);

        let alice_count = result.players.iter().filter(|s| s.name == "Alice").count();
        assert_eq!(alice_count, 1, "same name with different IDs must merge");

        let alice = result.players.iter().find(|s| s.name == "Alice").unwrap();
        assert_eq!(alice.hands_at_table, 2);
        assert_eq!(alice.vpip_hands, 2);
    }

    #[test]
    fn same_name_different_ids_net_merges() {
        let h1 = HandBuilder::new()
            .number(1)
            .player("session1_id", 1, "Pranav", 200.0)
            .player("p2", 2, "Bob", 200.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(1, 1.0)
            .bet(2, 3.0)
            .fold(1)
            .win(2, 4.0);

        let h2 = HandBuilder::new()
            .number(2)
            .player("session2_id", 1, "Pranav", 200.0)
            .player("p2", 2, "Bob", 200.0)
            .dealer(2)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .bet(1, 3.0)
            .fold(2)
            .win(1, 4.0);

        let data = parse_multi_game_data(&[&h1, &h2]);
        let result = compute_stats(&data);

        let pranav_entries: Vec<_> = result.players.iter().filter(|s| s.name == "Pranav").collect();
        assert_eq!(pranav_entries.len(), 1);

        let pranav = &pranav_entries[0];
        assert_eq!(pranav.hands_played, 2);
        // h1: BB=1.0, bet(3.0) by Bob → fold → invested max(1.0)=1.0, won 0 → net -1.0
        // h2: SB=0.5, bet(3.0) → Bob folds → invested max(0.5,3.0)=3.0, won 4.0 → net +1.0
        let expected_net = -1.0 + 1.0;
        assert!(
            (pranav.net_chips - expected_net).abs() < 0.01,
            "net_chips={}, expected={}",
            pranav.net_chips,
            expected_net,
        );
    }

    // Regression test: with --seed fixed, two full compute_stats runs over the
    // same session produce bit-for-bit identical all-in EV numbers. This is
    // the user-visible guarantee of `--seed 42`.
    #[test]
    fn seeded_compute_stats_is_reproducible() {
        let b = preflop_allin_hand(1.0, 100.0, true);
        let data = parse_game_data(&b);

        let cfg = EvConfig::deterministic(42);
        let r1 = compute_stats_with_ev_config(&data, &cfg);
        let r2 = compute_stats_with_ev_config(&data, &cfg);

        for (p1, p2) in r1.players.iter().zip(r2.players.iter()) {
            assert_eq!(p1.name, p2.name);
            assert_eq!(
                p1.all_in_ev_diff.to_bits(),
                p2.all_in_ev_diff.to_bits(),
                "seeded all_in_ev_diff must match bit-for-bit for {}",
                p1.name
            );
            assert_eq!(
                p1.all_in_ev_diff_chips.to_bits(),
                p2.all_in_ev_diff_chips.to_bits(),
                "seeded all_in_ev_diff_chips must match bit-for-bit for {}",
                p1.name
            );
        }
    }

    // Convergence test: 10 independent seeded runs on a preflop all-in hand
    // must give a tight spread on the reported EV (equity-driven). The
    // threshold follows the task spec — stddev of EV / pot_size < 0.5%.
    #[test]
    fn compute_stats_ev_converges_across_seeds() {
        let b = preflop_allin_hand(1.0, 100.0, true);
        let data = parse_game_data(&b);
        let pot_bb = 200.0; // both players all-in for 100 BB each

        let values: Vec<f64> = (0..10u64)
            .map(|i| {
                let cfg = EvConfig::deterministic(0xABCD_0000 + i);
                let r = compute_stats_with_ev_config(&data, &cfg);
                r.players.iter().find(|p| p.name == "Alice").unwrap().all_in_ev_diff
            })
            .collect();

        let mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
        let var: f64 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let sd = var.sqrt();
        let threshold = 0.005 * pot_bb; // 0.5% of pot per spec → 1.0 BB
        assert!(
            sd < threshold,
            "stddev of EV across 10 seeded runs = {sd:.4} BB, threshold = {threshold:.4} BB"
        );
    }
}
