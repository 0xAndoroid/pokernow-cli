use std::collections::HashSet;

use crate::ev::EvConfig;
use crate::parser::GameData;
use crate::search::hand_pot_bb;
use crate::stats::compute_stats_with_ev_config;

pub fn print_summary(data: &GameData, use_chips: bool) {
    print_summary_with_ev_config(data, use_chips, &EvConfig::default());
}

pub fn print_summary_with_ev_config(data: &GameData, use_chips: bool, cfg: &EvConfig) {
    let hand_count = data.hands.len();
    if hand_count == 0 {
        println!("No hands to summarize.");
        return;
    }

    let stakes: String = {
        let mut levels: Vec<(f64, f64)> = Vec::new();
        for h in &data.hands {
            let pair = (h.small_blind, h.big_blind);
            if !levels.iter().any(|(s, b)| {
                (*s - pair.0).abs() < f64::EPSILON && (*b - pair.1).abs() < f64::EPSILON
            }) {
                levels.push(pair);
            }
        }
        levels
            .iter()
            .map(|(sb, bb)| format!("{}/{}", format_chips(*sb), format_chips(*bb)))
            .collect::<Vec<_>>()
            .join(", ")
    };

    println!("Session Summary");
    println!("===============");
    println!("{hand_count} hands | Stakes: {stakes} | {} players", data.player_names.len());
    println!();

    let biggest = data.hands.iter().max_by(|a, b| hand_pot_bb(a).total_cmp(&hand_pot_bb(b)));
    if let Some(h) = biggest {
        let pot_display = if use_chips {
            let total: f64 = h.winners.iter().map(|w| w.amount).sum();
            format_chips(total)
        } else {
            format!("{:.1} BB", hand_pot_bb(h))
        };
        let mut seen = HashSet::new();
        let winner_names: Vec<&str> = h
            .winners
            .iter()
            .filter_map(|w| {
                if seen.insert(w.seat) {
                    h.players.iter().find(|p| p.seat == w.seat).map(|p| p.name.as_str())
                } else {
                    None
                }
            })
            .collect();
        println!("Biggest pot: {} (Hand #{}) — {}", pot_display, h.number, winner_names.join(", "));
    }

    let result = compute_stats_with_ev_config(data, cfg);
    println!();

    let pnl_header = if use_chips { "P&L" } else { "P&L (BB)" };
    let bbh_header = if use_chips { "$/hand" } else { "BB/hand" };
    println!("{:<16} {:>10} {:>8} {:>8} {:>10}", "Player", pnl_header, "VPIP", "PFR", bbh_header);
    println!("{}", "-".repeat(56));
    for s in &result.players {
        let (pnl_val, bbh_val) = if use_chips {
            let per_hand =
                if s.hands_at_table > 0 { s.net_chips / f64::from(s.hands_at_table) } else { 0.0 };
            let pnl = if s.net_chips >= 0.0 {
                format!("+{:.1}", s.net_chips)
            } else {
                format!("{:.1}", s.net_chips)
            };
            let bbh =
                if per_hand >= 0.0 { format!("+{per_hand:.2}") } else { format!("{per_hand:.2}") };
            (pnl, bbh)
        } else {
            let bb_per_hand =
                if s.hands_at_table > 0 { s.net_bb / f64::from(s.hands_at_table) } else { 0.0 };
            let pnl = if s.net_bb >= 0.0 {
                format!("+{:.1}", s.net_bb)
            } else {
                format!("{:.1}", s.net_bb)
            };
            let bbh = if bb_per_hand >= 0.0 {
                format!("+{bb_per_hand:.2}")
            } else {
                format!("{bb_per_hand:.2}")
            };
            (pnl, bbh)
        };
        let vpip = fmt_pct(s.vpip_hands, s.hands_played);
        let pfr = fmt_pct(s.pfr_hands, s.hands_played);
        println!("{:<16} {:>10} {:>8} {:>8} {:>10}", s.name, pnl_val, vpip, pfr, bbh_val);
    }

    if let Some(winner) = result.players.first() {
        println!();
        if use_chips {
            println!("Biggest winner: {} ({:+.1})", winner.name, winner.net_chips);
        } else {
            println!("Biggest winner: {} ({:+.1} BB)", winner.name, winner.net_bb);
        }
    }
    if let Some(loser) = result.players.last()
        && loser.net_bb < 0.0
    {
        if use_chips {
            println!("Biggest loser:  {} ({:.1})", loser.name, loser.net_chips);
        } else {
            println!("Biggest loser:  {} ({:.1} BB)", loser.name, loser.net_bb);
        }
    }
}

fn fmt_pct(num: u32, den: u32) -> String {
    if den == 0 {
        "-".to_string()
    } else {
        format!("{:.0}%", f64::from(num) / f64::from(den) * 100.0)
    }
}

fn format_chips(amount: f64) -> String {
    crate::format_chips(amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_helpers::*;

    #[test]
    fn summary_no_panic() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let data = parse_game_data(&b);
        print_summary(&data, false);
    }

    #[test]
    fn summary_empty_data() {
        let data = GameData {
            hands: Vec::new(),
            player_names: std::collections::HashMap::new(),
        };
        print_summary(&data, false);
    }

    #[test]
    fn summary_mixed_blinds_no_panic() {
        let h1 = HandBuilder::new()
            .blinds(0.5, 1.0)
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let h2 = HandBuilder::new()
            .blinds(1.0, 2.0)
            .number(2)
            .player("p1", 1, "Alice", 200.0)
            .player("p2", 2, "Bob", 200.0)
            .dealer(1)
            .sb(1, 1.0)
            .bb(2, 2.0)
            .fold(1)
            .win(2, 3.0);

        let data = parse_multi_game_data(&[&h1, &h2]);
        print_summary(&data, false);
    }

    #[test]
    fn summary_all_losers_no_double_sign() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .call(1, 1.0)
            .check(2)
            .win(1, 1.5);

        let data = parse_game_data(&b);
        print_summary(&data, false);
    }

    #[test]
    fn summary_chips_mode_no_panic() {
        let b = HandBuilder::new()
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let data = parse_game_data(&b);
        print_summary(&data, true);
    }

    #[test]
    fn summary_split_pot_shows_both_winners() {
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
            .showdown()
            .show(1, &["9s", "8s"])
            .show(2, &["9c", "8c"])
            .win(1, 1.0)
            .win(2, 1.0);

        let data = parse_game_data(&b);
        print_summary(&data, false);
    }
}
