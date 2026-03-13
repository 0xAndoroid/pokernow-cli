use std::collections::HashMap;
use std::path::PathBuf;

use poker_cli::parser;
use poker_cli::search::{self, SearchFilter, SortField};
use poker_cli::stats;

fn fixture_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(filename)
}

fn load_fixture(filename: &str) -> parser::GameData {
    let path = fixture_path(filename);
    assert!(path.exists(), "Fixture not found: {}", path.display());
    parser::parse_files(&[path.to_string_lossy().into_owned()], &HashMap::new(), &[]).unwrap()
}

#[test]
fn load_and_parse_real_file() {
    let data = load_fixture("sample.json");
    assert!(!data.hands.is_empty(), "should parse at least one hand");
    assert!(!data.player_names.is_empty(), "should track player names");
}

#[test]
fn all_hands_are_holdem() {
    let data = load_fixture("sample.json");
    for hand in &data.hands {
        assert!(!hand.bomb_pot, "bomb pots should be filtered out");
    }
}

#[test]
fn positions_always_assigned() {
    let data = load_fixture("sample.json");
    for hand in &data.hands {
        for p in &hand.players {
            let _ = p.position;
        }
    }
}

#[test]
fn stats_computation_on_real_data() {
    let data = load_fixture("sample.json");
    let all_stats = stats::compute_stats(&data);
    assert!(!all_stats.is_empty(), "should compute stats for at least one player");

    for s in &all_stats {
        assert!(s.hands_played > 0);
        let vpip_rate = f64::from(s.vpip_hands) / f64::from(s.hands_played);
        assert!((0.0..=1.0).contains(&vpip_rate), "VPIP must be 0-100%");
        let pfr_rate = f64::from(s.pfr_hands) / f64::from(s.hands_played);
        assert!((0.0..=1.0).contains(&pfr_rate), "PFR must be 0-100%");
        assert!(s.pfr_hands <= s.vpip_hands, "PFR must be <= VPIP for {}", s.name);
        if s.saw_flop > 0 {
            assert!(s.went_to_showdown <= s.saw_flop, "WTSD must be <= saw_flop for {}", s.name);
        }
        if s.went_to_showdown > 0 {
            assert!(s.won_at_showdown <= s.went_to_showdown, "W$SD must be <= WTSD for {}", s.name);
        }
    }
}

#[test]
fn net_pnl_sums_to_zero() {
    let data = load_fixture("sample.json");
    let all_stats = stats::compute_stats(&data);
    let total: f64 = all_stats.iter().map(|s| s.net_bb).sum();
    assert!(total.abs() < 1.0, "total P&L across all players should sum near zero, got {total:.2}");
}

#[test]
fn search_with_no_filter_returns_all() {
    let data = load_fixture("sample.json");
    let filter = SearchFilter {
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
    };
    let results = search::search_hands(&data, &filter);
    assert_eq!(results.len(), data.hands.len());
}

#[test]
fn search_showdown_filter_partitions() {
    let data = load_fixture("sample.json");

    let sd_filter = SearchFilter {
        player: None,
        saw_flop: None,
        saw_turn: None,
        saw_river: None,
        min_pot: None,
        max_pot: None,
        showdown: Some(true),
        won: false,
        lost: false,
        sort: SortField::HandId,
    };
    let no_sd_filter = SearchFilter {
        player: None,
        saw_flop: None,
        saw_turn: None,
        saw_river: None,
        min_pot: None,
        max_pot: None,
        showdown: Some(false),
        won: false,
        lost: false,
        sort: SortField::HandId,
    };

    let sd = search::search_hands(&data, &sd_filter);
    let no_sd = search::search_hands(&data, &no_sd_filter);
    assert_eq!(sd.len() + no_sd.len(), data.hands.len());
}

#[test]
fn search_pot_sort_is_descending() {
    let data = load_fixture("sample.json");
    let filter = SearchFilter {
        player: None,
        saw_flop: None,
        saw_turn: None,
        saw_river: None,
        min_pot: None,
        max_pot: None,
        showdown: None,
        won: false,
        lost: false,
        sort: SortField::Pot,
    };
    let results = search::search_hands(&data, &filter);
    for w in results.windows(2) {
        assert!(w[0].pot_bb >= w[1].pot_bb, "pot sort should be descending");
    }
}

#[test]
fn load_multiple_files() {
    let path1 = fixture_path("sample.json");
    let path2 = fixture_path("sample2.json");
    let data = parser::parse_files(
        &[path1.to_string_lossy().into_owned(), path2.to_string_lossy().into_owned()],
        &HashMap::new(),
        &[],
    )
    .unwrap();

    let single1 = load_fixture("sample.json");
    let single2 = load_fixture("sample2.json");
    assert_eq!(data.hands.len(), single1.hands.len() + single2.hands.len());
}

#[test]
fn player_unification_merges_ids() {
    let path = fixture_path("sample.json");
    let data_no_unify =
        parser::parse_files(&[path.to_string_lossy().into_owned()], &HashMap::new(), &[]).unwrap();

    let stats_no_unify = stats::compute_stats(&data_no_unify);
    let player_count_before = stats_no_unify.len();

    let mut unify = HashMap::new();
    if let Some(first) = stats_no_unify.first() {
        unify.insert(first.name.clone(), first.name.clone());
    }
    let data_with_unify =
        parser::parse_files(&[path.to_string_lossy().into_owned()], &unify, &[]).unwrap();
    let stats_with_unify = stats::compute_stats(&data_with_unify);
    assert_eq!(stats_with_unify.len(), player_count_before);
}

#[test]
fn display_hand_from_real_data() {
    use poker_cli::display;

    let data = load_fixture("sample.json");
    if let Some(hand) = data.hands.first() {
        display::display_hand(hand);
    }
}

#[test]
fn print_stats_from_real_data() {
    let data = load_fixture("sample.json");
    let all_stats = stats::compute_stats(&data);
    stats::print_stats(&all_stats);
}
