use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Card(u8);

impl Card {
    pub fn new(rank: u8, suit: u8) -> Self {
        Self(rank * 4 + suit)
    }

    pub fn rank(self) -> u8 {
        self.0 / 4
    }

    pub fn suit(self) -> u8 {
        self.0 % 4
    }

    pub fn parse(s: &str) -> Option<Card> {
        let mut chars = s.chars();
        let rank_ch = chars.next()?;
        let suit_ch = chars.next()?;
        if chars.next().is_some() {
            return None;
        }

        let rank = match rank_ch {
            '2'..='9' => rank_ch as u8 - b'0',
            'T' | 't' => 10,
            'J' | 'j' => 11,
            'Q' | 'q' => 12,
            'K' | 'k' => 13,
            'A' | 'a' => 14,
            _ => return None,
        };

        let suit = match suit_ch {
            'c' | 'C' => 0,
            'd' | 'D' => 1,
            'h' | 'H' => 2,
            's' | 'S' => 3,
            _ => return None,
        };

        Some(Card::new(rank, suit))
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", rank_char(self.rank()), suit_char(self.suit()))
    }
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

pub fn rank_char(rank: u8) -> char {
    match rank {
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'T',
        11 => 'J',
        12 => 'Q',
        13 => 'K',
        14 => 'A',
        _ => '?',
    }
}

pub fn rank_name(rank: u8) -> &'static str {
    match rank {
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        6 => "six",
        7 => "seven",
        8 => "eight",
        9 => "nine",
        10 => "ten",
        11 => "jack",
        12 => "queen",
        13 => "king",
        14 => "ace",
        _ => "unknown",
    }
}

pub fn suit_char(suit: u8) -> char {
    match suit {
        0 => 'c',
        1 => 'd',
        2 => 'h',
        3 => 's',
        _ => '?',
    }
}

fn rank_name_plural(rank: u8) -> &'static str {
    match rank {
        2 => "twos",
        3 => "threes",
        4 => "fours",
        5 => "fives",
        6 => "sixes",
        7 => "sevens",
        8 => "eights",
        9 => "nines",
        10 => "tens",
        11 => "jacks",
        12 => "queens",
        13 => "kings",
        14 => "aces",
        _ => "unknown",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum HandCategory {
    HighCard = 0,
    OnePair = 1,
    TwoPair = 2,
    ThreeOfAKind = 3,
    Straight = 4,
    Flush = 5,
    FullHouse = 6,
    FourOfAKind = 7,
    StraightFlush = 8,
}

#[derive(Clone, PartialEq, Eq)]
pub struct HandRank {
    pub score: u32,
    pub category: HandCategory,
    pub details: [u8; 5],
}

impl HandRank {
    pub fn worst() -> Self {
        Self {
            score: 0,
            category: HandCategory::HighCard,
            details: [0; 5],
        }
    }
}

impl Ord for HandRank {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score.cmp(&other.score)
    }
}

impl PartialOrd for HandRank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for HandRank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hand_description(self))
    }
}

fn pack_score(category: HandCategory, details: [u8; 5]) -> u32 {
    (category as u32) << 20
        | u32::from(details[0]) << 16
        | u32::from(details[1]) << 12
        | u32::from(details[2]) << 8
        | u32::from(details[3]) << 4
        | u32::from(details[4])
}

fn make_hand(category: HandCategory, details: [u8; 5]) -> HandRank {
    HandRank {
        score: pack_score(category, details),
        category,
        details,
    }
}

fn eval5(cards: [Card; 5]) -> HandRank {
    let mut ranks: [u8; 5] =
        [cards[0].rank(), cards[1].rank(), cards[2].rank(), cards[3].rank(), cards[4].rank()];
    ranks.sort_unstable_by(|a, b| b.cmp(a));

    let flush = cards[0].suit() == cards[1].suit()
        && cards[1].suit() == cards[2].suit()
        && cards[2].suit() == cards[3].suit()
        && cards[3].suit() == cards[4].suit();

    let unique = {
        let mut u = 1u8;
        for i in 1..5 {
            if ranks[i] != ranks[i - 1] {
                u += 1;
            }
        }
        u
    };

    let straight = if unique == 5 {
        if ranks[0] - ranks[4] == 4 {
            Some(ranks[0])
        } else if ranks[0] == 14 && ranks[1] == 5 && ranks[4] == 2 {
            Some(5u8) // wheel: high card is 5
        } else {
            None
        }
    } else {
        None
    };

    // (count, rank) groups sorted by count desc then rank desc
    let mut counts = [0u8; 15];
    for &r in &ranks {
        counts[r as usize] += 1;
    }
    let mut groups: [(u8, u8); 5] = [(0, 0); 5];
    let mut glen = 0usize;
    for r in (2..=14).rev() {
        if counts[r] > 0 {
            groups[glen] = (counts[r], r as u8);
            glen += 1;
        }
    }
    groups[..glen].sort_unstable_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    if let Some(high) = straight.filter(|_| flush) {
        return make_hand(HandCategory::StraightFlush, [high, 0, 0, 0, 0]);
    }

    if groups[0].0 == 4 {
        return make_hand(HandCategory::FourOfAKind, [groups[0].1, groups[1].1, 0, 0, 0]);
    }

    if groups[0].0 == 3 && groups[1].0 == 2 {
        return make_hand(HandCategory::FullHouse, [groups[0].1, groups[1].1, 0, 0, 0]);
    }

    if flush {
        return make_hand(HandCategory::Flush, ranks);
    }

    if let Some(high) = straight {
        return make_hand(HandCategory::Straight, [high, 0, 0, 0, 0]);
    }

    if groups[0].0 == 3 {
        return make_hand(
            HandCategory::ThreeOfAKind,
            [groups[0].1, groups[1].1, groups[2].1, 0, 0],
        );
    }

    if groups[0].0 == 2 && groups[1].0 == 2 {
        return make_hand(HandCategory::TwoPair, [groups[0].1, groups[1].1, groups[2].1, 0, 0]);
    }

    if groups[0].0 == 2 {
        return make_hand(
            HandCategory::OnePair,
            [groups[0].1, groups[1].1, groups[2].1, groups[3].1, 0],
        );
    }

    make_hand(HandCategory::HighCard, ranks)
}

pub fn evaluate(cards: &[Card]) -> HandRank {
    let n = cards.len();
    assert!(n >= 5, "evaluate requires at least 5 cards");

    let mut best = HandRank::worst();
    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                for l in (k + 1)..n {
                    for m in (l + 1)..n {
                        let hand = eval5([cards[i], cards[j], cards[k], cards[l], cards[m]]);
                        if hand.score > best.score {
                            best = hand;
                        }
                    }
                }
            }
        }
    }
    best
}

pub fn evaluate_omaha(hole: &[Card], board: &[Card]) -> HandRank {
    let mut best = HandRank::worst();
    let hn = hole.len();
    let bn = board.len();
    for hi in 0..hn {
        for hj in (hi + 1)..hn {
            for bi in 0..bn {
                for bj in (bi + 1)..bn {
                    for bk in (bj + 1)..bn {
                        let hand = eval5([hole[hi], hole[hj], board[bi], board[bj], board[bk]]);
                        if hand.score > best.score {
                            best = hand;
                        }
                    }
                }
            }
        }
    }
    best
}

pub fn hand_description(rank: &HandRank) -> String {
    let d = &rank.details;
    match rank.category {
        HandCategory::HighCard => format!("{}-high", rank_name(d[0])),
        HandCategory::OnePair => format!("pair of {}", rank_name_plural(d[0])),
        HandCategory::TwoPair => {
            format!("two pair, {} and {}", rank_name_plural(d[0]), rank_name_plural(d[1]))
        }
        HandCategory::ThreeOfAKind => {
            format!("three of a kind, {}", rank_name_plural(d[0]))
        }
        HandCategory::Straight => {
            if d[0] == 14 {
                "straight, ace to ten".to_string()
            } else if d[0] == 5 {
                "straight, ace to five".to_string()
            } else {
                format!("straight, {} to {}", rank_name(d[0]), rank_name(d[0] - 4))
            }
        }
        HandCategory::Flush => format!("flush, {}-high", rank_name(d[0])),
        HandCategory::FullHouse => {
            format!("full house, {} full of {}", rank_name_plural(d[0]), rank_name_plural(d[1]))
        }
        HandCategory::FourOfAKind => {
            format!("four of a kind, {}", rank_name_plural(d[0]))
        }
        HandCategory::StraightFlush => {
            if d[0] == 14 {
                "royal flush".to_string()
            } else {
                format!("straight flush, {}-high", rank_name(d[0]))
            }
        }
    }
}

pub fn holding_description(hole: &[Card], board: &[Card]) -> String {
    if board.is_empty() {
        return preflop_description(hole);
    }

    if hole.len() < 2 || (hole.len() > 2 && board.len() < 3) {
        return hole.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
    }

    let is_omaha = hole.len() > 2;
    let rank = if is_omaha {
        evaluate_omaha(hole, board)
    } else {
        let mut all = Vec::with_capacity(hole.len() + board.len());
        all.extend_from_slice(hole);
        all.extend_from_slice(board);
        evaluate(&all)
    };

    let base = if is_omaha {
        hand_description(&rank)
    } else {
        postflop_base_description(hole, board, &rank)
    };

    let draws = if !is_omaha && board.len() >= 3 && board.len() <= 4 {
        detect_draws(hole, board, &rank)
    } else {
        Vec::new()
    };

    if draws.is_empty() { base } else { format!("{} ({})", base, draws.join(", ")) }
}

fn preflop_description(hole: &[Card]) -> String {
    if hole.len() != 2 {
        return hole.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
    }

    let (a, b) = (hole[0], hole[1]);
    let (r0, r1) = if a.rank() >= b.rank() { (a.rank(), b.rank()) } else { (b.rank(), a.rank()) };
    let suited = a.suit() == b.suit();

    if r0 == r1 {
        return format!("pocket {}", rank_name_plural(r0));
    }

    let suffix = if suited { " suited" } else { " offsuit" };
    format!("{}-{}{}", rank_char(r0), rank_char(r1), suffix)
}

fn postflop_base_description(hole: &[Card], board: &[Card], rank: &HandRank) -> String {
    match rank.category {
        HandCategory::OnePair => describe_pair(hole, board, rank.details),
        HandCategory::ThreeOfAKind => describe_trips(hole, rank.details),
        HandCategory::HighCard
        | HandCategory::TwoPair
        | HandCategory::Straight
        | HandCategory::Flush
        | HandCategory::FullHouse
        | HandCategory::FourOfAKind
        | HandCategory::StraightFlush => hand_description(rank),
    }
}

fn describe_pair(hole: &[Card], board: &[Card], d: [u8; 5]) -> String {
    let pair_rank = d[0];

    let hole_has_pair =
        hole.len() == 2 && hole[0].rank() == hole[1].rank() && hole[0].rank() == pair_rank;

    if hole_has_pair {
        let board_ranks: Vec<u8> = board.iter().map(|c| c.rank()).collect();
        let board_max = board_ranks.iter().copied().max().unwrap_or(0);
        if pair_rank > board_max {
            return format!("overpair, pocket {}", rank_name_plural(pair_rank));
        }
        return format!("pocket pair, {}", rank_name_plural(pair_rank));
    }

    let hole_contributes =
        hole.iter().any(|c| c.rank() == pair_rank) && board.iter().any(|c| c.rank() == pair_rank);

    if hole_contributes {
        let mut board_ranks: Vec<u8> = board.iter().map(|c| c.rank()).collect();
        board_ranks.sort_unstable_by(|a, b| b.cmp(a));
        board_ranks.dedup();

        if !board_ranks.is_empty() && board_ranks[0] == pair_rank {
            return format!("top pair, {}", rank_name_plural(pair_rank));
        }
        if board_ranks.len() >= 2 && board_ranks.last().copied() == Some(pair_rank) {
            return format!("bottom pair, {}", rank_name_plural(pair_rank));
        }
        return format!("middle pair, {}", rank_name_plural(pair_rank));
    }

    format!("pair of {}", rank_name_plural(pair_rank))
}

fn describe_trips(hole: &[Card], d: [u8; 5]) -> String {
    let trips_rank = d[0];
    let hole_pair =
        hole.len() == 2 && hole[0].rank() == hole[1].rank() && hole[0].rank() == trips_rank;
    if hole_pair {
        format!("set of {}", rank_name_plural(trips_rank))
    } else {
        format!("trip {}", rank_name_plural(trips_rank))
    }
}

fn detect_draws(hole: &[Card], board: &[Card], _rank: &HandRank) -> Vec<String> {
    let mut draws = Vec::new();

    let mut all_cards = Vec::with_capacity(hole.len() + board.len());
    all_cards.extend_from_slice(hole);
    all_cards.extend_from_slice(board);

    if let Some(fd) = detect_flush_draw(hole, &all_cards) {
        draws.push(fd);
    }

    if let Some(sd) = detect_straight_draw(&all_cards) {
        draws.push(sd);
    }

    draws
}

fn detect_flush_draw(hole: &[Card], all: &[Card]) -> Option<String> {
    let mut suit_counts = [0u8; 4];
    for c in all {
        suit_counts[c.suit() as usize] += 1;
    }

    for suit in 0..4u8 {
        if suit_counts[suit as usize] == 4 {
            let hole_has_suit = hole.iter().any(|c| c.suit() == suit);
            if !hole_has_suit {
                continue;
            }
            let max_hole_rank_in_suit =
                hole.iter().filter(|c| c.suit() == suit).map(|c| c.rank()).max().unwrap();
            if max_hole_rank_in_suit == 14 {
                return Some("nut flush draw".to_string());
            }
            return Some("flush draw".to_string());
        }
    }
    None
}

fn detect_straight_draw(all: &[Card]) -> Option<String> {
    let mut present = [false; 15];
    for c in all {
        present[c.rank() as usize] = true;
    }
    // Ace also counts as 1 for low straights
    if present[14] {
        present[1] = true;
    }

    // Check all windows of 5 consecutive ranks for OESD and gutshots
    let mut oesd = false;
    let mut gutshot = false;

    for low in 1..=10u8 {
        let high = low + 4;
        let count = (low..=high).filter(|&r| present[r as usize]).count();

        // Already have a straight with these 5 — skip
        if count == 5 {
            continue;
        }

        if count == 4 {
            let missing = (low..=high).find(|&r| !present[r as usize]).unwrap();
            if missing == low || missing == high {
                oesd = true;
            } else {
                gutshot = true;
            }
        }
    }

    if oesd {
        Some("open-ended straight draw".to_string())
    } else if gutshot {
        Some("gutshot straight draw".to_string())
    } else {
        None
    }
}
