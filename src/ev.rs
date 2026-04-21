use std::collections::HashSet;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

use crate::card::{Card, evaluate};
use crate::parser::Hand;

// Defaults sized so the MC estimator's standard error per pot slice is below
// ~0.2% of pot size (target_stderr operates on equity, which is already in
// fractional-of-pot units). That is a 2.5× tightening over the spec's 0.5%
// ceiling and keeps session-level EV stable to within ~1 BB for typical
// session sizes, while trivial spots converge in a single 20k chunk.
const DEFAULT_TARGET_STDERR: f64 = 0.002;
const DEFAULT_CHUNK_SIZE: usize = 20_000;
const DEFAULT_MAX_SAMPLES: usize = 2_000_000;
const DEFAULT_MIN_SAMPLES: usize = 20_000;

#[derive(Clone, Debug)]
pub struct EvConfig {
    /// `Some(seed)` → every MC stream is derived from this master seed,
    /// so a run is bit-for-bit reproducible. `None` → entropy-seeded.
    pub seed: Option<u64>,
    pub target_stderr: f64,
    pub chunk_size: usize,
    pub max_samples: usize,
    pub min_samples: usize,
    /// Split each chunk across rayon worker threads. Disable for tiny loads
    /// where thread-pool overhead dominates.
    pub parallel_chunks: bool,
}

impl Default for EvConfig {
    fn default() -> Self {
        Self {
            seed: None,
            target_stderr: DEFAULT_TARGET_STDERR,
            chunk_size: DEFAULT_CHUNK_SIZE,
            max_samples: DEFAULT_MAX_SAMPLES,
            min_samples: DEFAULT_MIN_SAMPLES,
            parallel_chunks: true,
        }
    }
}

impl EvConfig {
    pub fn deterministic(seed: u64) -> Self {
        Self {
            seed: Some(seed),
            ..Self::default()
        }
    }
}

// SplitMix64 — fast, high-quality 64-bit integer hash. Used to derive
// independent child seeds from a master seed + salt.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

pub fn mix_seed(master: u64, salt_a: u64, salt_b: u64) -> u64 {
    splitmix64(splitmix64(master.wrapping_add(salt_a)).wrapping_add(salt_b))
}

fn resolve_chunk_seed(cfg: &EvConfig, seed_salt: u64, chunk_idx: u64) -> u64 {
    if let Some(s) = cfg.seed {
        mix_seed(s, seed_salt, chunk_idx)
    } else {
        let fresh: u64 = rand::random();
        splitmix64(fresh ^ mix_seed(seed_salt, chunk_idx, 0))
    }
}

/// Compute multi-way equity for a contested pot slice. Callers pass a stable
/// `seed_salt` (e.g. derived from hand index + slice index) so that seeded
/// runs are deterministic even under rayon's unordered reductions.
pub fn calculate_multi_equity(
    players: &[(u8, &Vec<Card>)],
    board: &[Card],
    hand: &Hand,
    cfg: &EvConfig,
    seed_salt: u64,
) -> Vec<f64> {
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
        monte_carlo_equity_adaptive(players, board, &deck, cards_needed, cfg, seed_salt)
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
    let mut wins_sq = vec![0.0_f64; n]; // unused; lets us share tally_result.
    let mut total = 0u64;
    let dk = deck.len();
    let mut full_board = Vec::with_capacity(5);
    let mut scores = Vec::with_capacity(n);
    let mut combined = Vec::with_capacity(7);

    if cards_needed == 1 {
        for card in deck {
            full_board.clear();
            full_board.extend_from_slice(board);
            full_board.push(*card);
            tally_result(players, &full_board, &mut wins, &mut wins_sq, &mut scores, &mut combined);
            total += 1;
        }
    } else {
        for i in 0..dk {
            for j in (i + 1)..dk {
                full_board.clear();
                full_board.extend_from_slice(board);
                full_board.push(deck[i]);
                full_board.push(deck[j]);
                tally_result(
                    players,
                    &full_board,
                    &mut wins,
                    &mut wins_sq,
                    &mut scores,
                    &mut combined,
                );
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

#[derive(Clone)]
struct ChunkResult {
    wins_sum: Vec<f64>,
    wins_sq_sum: Vec<f64>,
    samples: usize,
}

impl ChunkResult {
    fn empty(n: usize) -> Self {
        Self {
            wins_sum: vec![0.0; n],
            wins_sq_sum: vec![0.0; n],
            samples: 0,
        }
    }

    fn merge(mut self, other: Self) -> Self {
        if self.wins_sum.is_empty() {
            return other;
        }
        if other.wins_sum.is_empty() {
            return self;
        }
        for i in 0..self.wins_sum.len() {
            self.wins_sum[i] += other.wins_sum[i];
            self.wins_sq_sum[i] += other.wins_sq_sum[i];
        }
        self.samples += other.samples;
        self
    }
}

fn run_chunk(
    players: &[(u8, &Vec<Card>)],
    board: &[Card],
    deck: &[Card],
    cards_needed: usize,
    samples: usize,
    seed: u64,
) -> ChunkResult {
    let n = players.len();
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut wins_sum = vec![0.0_f64; n];
    let mut wins_sq_sum = vec![0.0_f64; n];
    let dk = deck.len();
    let mut full_board = Vec::with_capacity(5);
    let mut scores = Vec::with_capacity(n);
    let mut combined = Vec::with_capacity(7);
    let mut used = [false; 52];

    for _ in 0..samples {
        full_board.clear();
        full_board.extend_from_slice(board);
        used.fill(false);

        let mut drawn = 0;
        while drawn < cards_needed {
            let idx = rng.gen_range(0..dk);
            if !used[idx] {
                used[idx] = true;
                full_board.push(deck[idx]);
                drawn += 1;
            }
        }

        tally_result(
            players,
            &full_board,
            &mut wins_sum,
            &mut wins_sq_sum,
            &mut scores,
            &mut combined,
        );
    }

    ChunkResult {
        wins_sum,
        wins_sq_sum,
        samples,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_chunk_parallel(
    players: &[(u8, &Vec<Card>)],
    board: &[Card],
    deck: &[Card],
    cards_needed: usize,
    total_samples: usize,
    cfg: &EvConfig,
    seed_salt: u64,
    chunk_idx: u64,
) -> ChunkResult {
    let threads = rayon::current_num_threads().max(1);
    let per_thread = total_samples.div_ceil(threads);
    let mut partitions: Vec<usize> = Vec::with_capacity(threads);
    let mut remaining = total_samples;
    for _ in 0..threads {
        if remaining == 0 {
            break;
        }
        let size = per_thread.min(remaining);
        partitions.push(size);
        remaining -= size;
    }

    let n = players.len();
    partitions
        .par_iter()
        .enumerate()
        .map(|(sub_idx, &size)| {
            // Mix sub_idx so independent workers get disjoint streams even
            // within a single chunk, and two slices within the same hand
            // don't collide.
            let sub_salt = mix_seed(seed_salt, chunk_idx, sub_idx as u64);
            let seed = resolve_chunk_seed(cfg, sub_salt, chunk_idx);
            run_chunk(players, board, deck, cards_needed, size, seed)
        })
        .reduce(|| ChunkResult::empty(n), ChunkResult::merge)
}

fn monte_carlo_equity_adaptive(
    players: &[(u8, &Vec<Card>)],
    board: &[Card],
    deck: &[Card],
    cards_needed: usize,
    cfg: &EvConfig,
    seed_salt: u64,
) -> Vec<f64> {
    let n = players.len();
    let mut acc = ChunkResult::empty(n);
    let mut chunk_idx: u64 = 0;

    while acc.samples < cfg.max_samples {
        let remaining = cfg.max_samples - acc.samples;
        let this_chunk = cfg.chunk_size.min(remaining);

        let chunk = if cfg.parallel_chunks {
            run_chunk_parallel(
                players,
                board,
                deck,
                cards_needed,
                this_chunk,
                cfg,
                seed_salt,
                chunk_idx,
            )
        } else {
            let seed = resolve_chunk_seed(cfg, seed_salt, chunk_idx);
            run_chunk(players, board, deck, cards_needed, this_chunk, seed)
        };
        acc = acc.merge(chunk);
        chunk_idx += 1;

        if acc.samples >= cfg.min_samples && converged(&acc, cfg.target_stderr) {
            break;
        }
    }

    let total_f = acc.samples as f64;
    if total_f == 0.0 {
        return vec![1.0 / n as f64; n];
    }
    acc.wins_sum.iter().map(|w| w / total_f).collect()
}

fn converged(acc: &ChunkResult, target_stderr: f64) -> bool {
    let total_f = acc.samples as f64;
    for i in 0..acc.wins_sum.len() {
        let mean = acc.wins_sum[i] / total_f;
        let mean_sq = acc.wins_sq_sum[i] / total_f;
        let var = (mean_sq - mean * mean).max(0.0);
        let stderr_of_mean = (var / total_f).sqrt();
        if stderr_of_mean > target_stderr {
            return false;
        }
    }
    true
}

fn tally_result(
    players: &[(u8, &Vec<Card>)],
    board: &[Card],
    wins: &mut [f64],
    wins_sq: &mut [f64],
    scores: &mut Vec<u32>,
    combined: &mut Vec<Card>,
) {
    scores.clear();
    let mut best_score = 0u32;

    for (_, hole) in players {
        combined.clear();
        combined.extend_from_slice(hole);
        combined.extend_from_slice(board);
        let s = evaluate(combined).score;
        if s > best_score {
            best_score = s;
        }
        scores.push(s);
    }

    let mut winner_count = 0u32;
    for &s in scores.iter() {
        if s == best_score {
            winner_count += 1;
        }
    }

    let share = 1.0 / f64::from(winner_count);
    let share_sq = share * share;
    for (i, &s) in scores.iter().enumerate() {
        if s == best_score {
            wins[i] += share;
            wins_sq[i] += share_sq;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Card;

    fn card(s: &str) -> Card {
        Card::parse(s).unwrap()
    }

    fn hole(c1: &str, c2: &str) -> Vec<Card> {
        vec![card(c1), card(c2)]
    }

    fn empty_hand() -> Hand {
        Hand {
            id: String::new(),
            number: 0,
            small_blind: 0.5,
            big_blind: 1.0,
            effective_bb: 1.0,
            straddle_seat: None,
            bomb_pot: false,
            players: Vec::new(),
            streets: Vec::new(),
            winners: Vec::new(),
            real_showdown: false,
            shown_cards: std::collections::HashMap::new(),
            uncalled_returns: std::collections::HashMap::new(),
            run_it_twice: false,
            run2_cards: Vec::new(),
        }
    }

    #[test]
    fn seeded_runs_are_bitwise_identical() {
        let p1 = hole("Ah", "Ad");
        let p2 = hole("Kh", "Kd");
        let players: Vec<(u8, &Vec<Card>)> = vec![(1, &p1), (2, &p2)];
        let board: Vec<Card> = Vec::new();
        let hand = empty_hand();

        let cfg = EvConfig::deterministic(42);
        let eq1 = calculate_multi_equity(&players, &board, &hand, &cfg, 0);
        let eq2 = calculate_multi_equity(&players, &board, &hand, &cfg, 0);
        assert_eq!(eq1, eq2, "deterministic seed must yield identical equities");
        assert!((eq1[0] + eq1[1] - 1.0).abs() < 1e-9);
        assert!(eq1[0] > 0.75 && eq1[0] < 0.90, "AA vs KK preflop ~0.82");
    }

    #[test]
    fn different_seeds_are_close_but_not_equal() {
        let p1 = hole("Ah", "Ad");
        let p2 = hole("Kh", "Kd");
        let players: Vec<(u8, &Vec<Card>)> = vec![(1, &p1), (2, &p2)];
        let board: Vec<Card> = Vec::new();
        let hand = empty_hand();

        let eq_a = calculate_multi_equity(&players, &board, &hand, &EvConfig::deterministic(1), 0);
        let eq_b = calculate_multi_equity(&players, &board, &hand, &EvConfig::deterministic(2), 0);
        // Both should converge near true equity (~0.82).
        assert!((eq_a[0] - eq_b[0]).abs() < 0.01);
    }

    #[test]
    fn one_outer_convergence_across_seeds() {
        // AA vs 23o preflop is the textbook "one-outer"-adjacent stress
        // test: tiny-probability tail events dominate the underdog's
        // variance. 10 independent seeded runs must converge within a
        // tight spread on the underdog's equity.
        let p1 = hole("Ah", "Ad");
        let p2 = hole("2c", "3d");
        let players: Vec<(u8, &Vec<Card>)> = vec![(1, &p1), (2, &p2)];
        let board: Vec<Card> = Vec::new();
        let hand = empty_hand();

        let values: Vec<f64> = (0..10u64)
            .map(|s| {
                let cfg = EvConfig::deterministic(10_000 + s);
                calculate_multi_equity(&players, &board, &hand, &cfg, 0)[1]
            })
            .collect();
        let mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
        let var: f64 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let sd = var.sqrt();
        assert!(sd < 0.004, "equity stddev across 10 seeded runs = {sd:.4} (expected < 0.004)");
    }

    #[test]
    fn enumerate_used_when_two_cards_needed() {
        // Turn all-in: 1 card to come → enumerate, exact, no MC.
        let p1 = hole("Ah", "Ad");
        let p2 = hole("Kh", "Kd");
        let players: Vec<(u8, &Vec<Card>)> = vec![(1, &p1), (2, &p2)];
        let board: Vec<Card> = vec![card("2c"), card("5d"), card("8h"), card("9s")];
        let hand = empty_hand();
        let cfg = EvConfig::default();
        let eq = calculate_multi_equity(&players, &board, &hand, &cfg, 0);
        // AA holds with 44 non-K cards / 44 remaining: exactly 1.0 except for
        // the two kings (2/44 chop or swing). Just sanity-check reasonable
        // range.
        assert!((eq[0] + eq[1] - 1.0).abs() < 1e-9);
        assert!(eq[0] > 0.9);
    }

    #[test]
    fn serial_chunks_matches_parallel_seeded() {
        let p1 = hole("Ah", "Ad");
        let p2 = hole("Kh", "Kd");
        let players: Vec<(u8, &Vec<Card>)> = vec![(1, &p1), (2, &p2)];
        let board: Vec<Card> = Vec::new();
        let hand = empty_hand();

        let mut cfg_serial = EvConfig::deterministic(7);
        cfg_serial.parallel_chunks = false;
        cfg_serial.min_samples = 40_000;
        cfg_serial.target_stderr = 0.01;
        cfg_serial.max_samples = 40_000;

        let mut cfg_par = cfg_serial.clone();
        cfg_par.parallel_chunks = true;

        let eq_serial = calculate_multi_equity(&players, &board, &hand, &cfg_serial, 0);
        let eq_par = calculate_multi_equity(&players, &board, &hand, &cfg_par, 0);
        // Both should converge near the true value; they won't match exactly
        // because partitioning changes the stream layout.
        assert!((eq_serial[0] - eq_par[0]).abs() < 0.01);
        assert!((eq_serial[0] + eq_serial[1] - 1.0).abs() < 1e-9);
        assert!((eq_par[0] + eq_par[1] - 1.0).abs() < 1e-9);
    }
}
