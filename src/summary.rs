use crate::parser::GameData;
use crate::search::hand_pot_bb;
use crate::stats::compute_stats;

pub fn print_summary(data: &GameData) {
    let hand_count = data.hands.len();
    if hand_count == 0 {
        println!("No hands to summarize.");
        return;
    }

    let stakes: String = {
        let first = &data.hands[0];
        format!("{}/{}", format_chips(first.small_blind), format_chips(first.big_blind))
    };

    println!("Session Summary");
    println!("===============");
    println!("{hand_count} hands | Stakes: {stakes} | {} players", data.player_names.len());
    println!();

    let biggest = data.hands.iter().max_by(|a, b| hand_pot_bb(a).total_cmp(&hand_pot_bb(b)));
    if let Some(h) = biggest {
        println!("Biggest pot: {:.1} BB (Hand #{})", hand_pot_bb(h), h.number);
    }

    let stats = compute_stats(data);
    println!();
    println!("{:<16} {:>10} {:>8} {:>8} {:>10}", "Player", "P&L (BB)", "VPIP", "PFR", "BB/hand");
    println!("{}", "-".repeat(56));
    for s in &stats {
        let bb_per_hand =
            if s.hands_at_table > 0 { s.net_bb / f64::from(s.hands_at_table) } else { 0.0 };
        let vpip = fmt_pct(s.vpip_hands, s.hands_played);
        let pfr = fmt_pct(s.pfr_hands, s.hands_played);
        let pnl =
            if s.net_bb >= 0.0 { format!("+{:.1}", s.net_bb) } else { format!("{:.1}", s.net_bb) };
        let bbh = if bb_per_hand >= 0.0 {
            format!("+{bb_per_hand:.2}")
        } else {
            format!("{bb_per_hand:.2}")
        };
        println!("{:<16} {:>10} {:>8} {:>8} {:>10}", s.name, pnl, vpip, pfr, bbh);
    }

    if let Some(winner) = stats.first() {
        println!();
        println!("Biggest winner: {} (+{:.1} BB)", winner.name, winner.net_bb);
    }
    if let Some(loser) = stats.last()
        && loser.net_bb < 0.0
    {
        println!("Biggest loser:  {} ({:.1} BB)", loser.name, loser.net_bb);
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
        print_summary(&data);
    }

    #[test]
    fn summary_empty_data() {
        let data = GameData {
            hands: Vec::new(),
            player_names: std::collections::HashMap::new(),
        };
        print_summary(&data);
    }
}
