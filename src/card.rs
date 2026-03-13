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

fn detect_draws(hole: &[Card], board: &[Card], rank: &HandRank) -> Vec<String> {
    let mut draws = Vec::new();

    let mut all_cards = Vec::with_capacity(hole.len() + board.len());
    all_cards.extend_from_slice(hole);
    all_cards.extend_from_slice(board);

    if rank.category < HandCategory::Flush
        && let Some(fd) = detect_flush_draw(hole, &all_cards)
    {
        draws.push(fd);
    }

    if rank.category < HandCategory::Straight
        && let Some(sd) = detect_straight_draw(hole, &all_cards, board)
    {
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

fn detect_straight_draw(hole: &[Card], all: &[Card], board: &[Card]) -> Option<String> {
    let mut present = [false; 15];
    for c in all {
        present[c.rank() as usize] = true;
    }
    if present[14] {
        present[1] = true;
    }

    let mut board_present = [false; 15];
    for c in board {
        board_present[c.rank() as usize] = true;
    }
    if board_present[14] {
        board_present[1] = true;
    }

    let mut hole_ranks: Vec<u8> = hole.iter().map(|c| c.rank()).collect();
    if hole_ranks.contains(&14) {
        hole_ranks.push(1);
    }

    let mut oesd = false;
    let mut gutshot = false;

    for low in 1..=10u8 {
        let high = low + 4;
        let count = (low..=high).filter(|&r| present[r as usize]).count();
        if count == 5 {
            continue;
        }
        if count != 4 {
            continue;
        }

        // Board-only draw: all 4 present ranks come from the board alone.
        let board_count = (low..=high).filter(|&r| board_present[r as usize]).count();
        let hole_contributes = (low..=high).any(|r| present[r as usize] && hole_ranks.contains(&r));
        if board_count >= 4 || !hole_contributes {
            continue;
        }

        let missing = (low..=high).find(|&r| !present[r as usize]).unwrap();
        if missing == low || missing == high {
            oesd = true;
        } else {
            gutshot = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Card {
        Card::parse(s).unwrap()
    }

    fn cards(ss: &[&str]) -> Vec<Card> {
        ss.iter().map(|s| c(s)).collect()
    }

    // --- Card parsing ---

    #[test]
    fn parse_valid_cards() {
        let card = c("As");
        assert_eq!(card.rank(), 14);
        assert_eq!(card.suit(), 3);
        assert_eq!(card.to_string(), "As");

        let card = c("2c");
        assert_eq!(card.rank(), 2);
        assert_eq!(card.suit(), 0);

        let card = c("Td");
        assert_eq!(card.rank(), 10);
        assert_eq!(card.suit(), 1);

        let card = c("Kh");
        assert_eq!(card.rank(), 13);
        assert_eq!(card.suit(), 2);
    }

    #[test]
    fn parse_case_insensitive() {
        assert!(Card::parse("aS").is_some());
        assert!(Card::parse("tC").is_some());
        assert!(Card::parse("jD").is_some());
        assert!(Card::parse("qH").is_some());
    }

    #[test]
    fn parse_invalid_cards() {
        assert!(Card::parse("").is_none());
        assert!(Card::parse("A").is_none());
        assert!(Card::parse("1s").is_none());
        assert!(Card::parse("Ax").is_none());
        assert!(Card::parse("Asc").is_none());
    }

    #[test]
    fn card_new_roundtrip() {
        for rank in 2..=14u8 {
            for suit in 0..4u8 {
                let card = Card::new(rank, suit);
                assert_eq!(card.rank(), rank);
                assert_eq!(card.suit(), suit);
            }
        }
    }

    #[test]
    fn card_display() {
        assert_eq!(c("As").to_string(), "As");
        assert_eq!(c("Tc").to_string(), "Tc");
        assert_eq!(c("2d").to_string(), "2d");
    }

    #[test]
    fn rank_char_all() {
        assert_eq!(rank_char(2), '2');
        assert_eq!(rank_char(10), 'T');
        assert_eq!(rank_char(11), 'J');
        assert_eq!(rank_char(12), 'Q');
        assert_eq!(rank_char(13), 'K');
        assert_eq!(rank_char(14), 'A');
        assert_eq!(rank_char(0), '?');
    }

    #[test]
    fn suit_char_all() {
        assert_eq!(suit_char(0), 'c');
        assert_eq!(suit_char(1), 'd');
        assert_eq!(suit_char(2), 'h');
        assert_eq!(suit_char(3), 's');
        assert_eq!(suit_char(4), '?');
    }

    // --- Hand evaluation: all categories ---

    #[test]
    fn eval_high_card() {
        let hand = eval5([c("As"), c("Kd"), c("9h"), c("7c"), c("2s")]);
        assert_eq!(hand.category, HandCategory::HighCard);
        assert_eq!(hand.details[0], 14);
    }

    #[test]
    fn eval_one_pair() {
        let hand = eval5([c("Ks"), c("Kd"), c("9h"), c("7c"), c("2s")]);
        assert_eq!(hand.category, HandCategory::OnePair);
        assert_eq!(hand.details[0], 13);
    }

    #[test]
    fn eval_two_pair() {
        let hand = eval5([c("Ks"), c("Kd"), c("5h"), c("5c"), c("9s")]);
        assert_eq!(hand.category, HandCategory::TwoPair);
        assert_eq!(hand.details[0], 13);
        assert_eq!(hand.details[1], 5);
    }

    #[test]
    fn eval_three_of_a_kind() {
        let hand = eval5([c("7s"), c("7d"), c("7h"), c("Kc"), c("2s")]);
        assert_eq!(hand.category, HandCategory::ThreeOfAKind);
        assert_eq!(hand.details[0], 7);
    }

    #[test]
    fn eval_straight_normal() {
        let hand = eval5([c("9s"), c("8d"), c("7h"), c("6c"), c("5s")]);
        assert_eq!(hand.category, HandCategory::Straight);
        assert_eq!(hand.details[0], 9);
    }

    #[test]
    fn eval_wheel_straight() {
        let hand = eval5([c("5s"), c("4d"), c("3h"), c("2c"), c("As")]);
        assert_eq!(hand.category, HandCategory::Straight);
        assert_eq!(hand.details[0], 5);
    }

    #[test]
    fn eval_broadway_straight() {
        let hand = eval5([c("Ac"), c("Kd"), c("Qh"), c("Js"), c("Tc")]);
        assert_eq!(hand.category, HandCategory::Straight);
        assert_eq!(hand.details[0], 14);
    }

    #[test]
    fn eval_flush() {
        let hand = eval5([c("As"), c("Js"), c("9s"), c("5s"), c("3s")]);
        assert_eq!(hand.category, HandCategory::Flush);
        assert_eq!(hand.details[0], 14);
    }

    #[test]
    fn eval_full_house() {
        let hand = eval5([c("Ks"), c("Kd"), c("Kh"), c("5c"), c("5s")]);
        assert_eq!(hand.category, HandCategory::FullHouse);
        assert_eq!(hand.details[0], 13);
        assert_eq!(hand.details[1], 5);
    }

    #[test]
    fn eval_four_of_a_kind() {
        let hand = eval5([c("Qs"), c("Qd"), c("Qh"), c("Qc"), c("2s")]);
        assert_eq!(hand.category, HandCategory::FourOfAKind);
        assert_eq!(hand.details[0], 12);
    }

    #[test]
    fn eval_straight_flush() {
        let hand = eval5([c("9h"), c("8h"), c("7h"), c("6h"), c("5h")]);
        assert_eq!(hand.category, HandCategory::StraightFlush);
        assert_eq!(hand.details[0], 9);
    }

    #[test]
    fn eval_royal_flush() {
        let hand = eval5([c("As"), c("Ks"), c("Qs"), c("Js"), c("Ts")]);
        assert_eq!(hand.category, HandCategory::StraightFlush);
        assert_eq!(hand.details[0], 14);
    }

    #[test]
    fn eval_wheel_straight_flush() {
        let hand = eval5([c("5d"), c("4d"), c("3d"), c("2d"), c("Ad")]);
        assert_eq!(hand.category, HandCategory::StraightFlush);
        assert_eq!(hand.details[0], 5);
    }

    // --- Ranking comparisons ---

    #[test]
    fn hand_category_ordering() {
        let high_card = eval5([c("As"), c("Kd"), c("9h"), c("7c"), c("2s")]);
        let pair = eval5([c("Ks"), c("Kd"), c("9h"), c("7c"), c("2s")]);
        let flush = eval5([c("As"), c("Js"), c("9s"), c("5s"), c("3s")]);
        let full_house = eval5([c("Ks"), c("Kd"), c("Kh"), c("5c"), c("5s")]);
        let sf = eval5([c("9h"), c("8h"), c("7h"), c("6h"), c("5h")]);

        assert!(high_card < pair);
        assert!(pair < flush);
        assert!(flush < full_house);
        assert!(full_house < sf);
    }

    #[test]
    fn flush_beats_straight_not_straight_flush() {
        let straight = eval5([c("9s"), c("8d"), c("7h"), c("6c"), c("5s")]);
        let flush = eval5([c("As"), c("Js"), c("9s"), c("5s"), c("3s")]);
        let sf = eval5([c("9h"), c("8h"), c("7h"), c("6h"), c("5h")]);

        assert!(straight < flush);
        assert!(flush < sf);
        assert_eq!(straight.category, HandCategory::Straight);
        assert_eq!(flush.category, HandCategory::Flush);
        assert_eq!(sf.category, HandCategory::StraightFlush);
    }

    #[test]
    fn kicker_comparison() {
        let pair_k_a = eval5([c("Ks"), c("Kd"), c("Ah"), c("7c"), c("2s")]);
        let pair_k_q = eval5([c("Ks"), c("Kd"), c("Qh"), c("7c"), c("2s")]);
        assert!(pair_k_a > pair_k_q);
    }

    // --- evaluate() with 7 cards ---

    #[test]
    fn evaluate_7_cards_picks_best() {
        let all = cards(&["As", "Ks", "Qs", "Js", "Ts", "2c", "3d"]);
        let rank = evaluate(&all);
        assert_eq!(rank.category, HandCategory::StraightFlush);
        assert_eq!(rank.details[0], 14);
    }

    #[test]
    fn evaluate_6_cards() {
        let all = cards(&["As", "Ad", "Ah", "Kc", "Ks", "2c"]);
        let rank = evaluate(&all);
        assert_eq!(rank.category, HandCategory::FullHouse);
    }

    // --- Omaha evaluation ---

    #[test]
    fn evaluate_omaha_2_plus_3() {
        let hole = cards(&["As", "Kd", "Qs", "Jd"]);
        let board = cards(&["Ts", "9s", "8s", "2c", "3c"]);
        let rank = evaluate_omaha(&hole, &board);
        // Must use exactly 2 hole + 3 board. Best: As Qs + Ts 9s 8s = flush
        assert_eq!(rank.category, HandCategory::Flush);
    }

    #[test]
    fn omaha_cannot_use_three_hole_cards() {
        let hole = cards(&["As", "Ks", "Qs", "2d"]);
        let board = cards(&["Js", "Ts", "3h", "4h", "5h"]);
        // If we could use 3 hole: As Ks Qs Js Ts = royal flush
        // With 2+3 constraint, best is different
        let rank = evaluate_omaha(&hole, &board);
        assert_ne!(rank.category, HandCategory::StraightFlush);
    }

    #[test]
    fn omaha_cannot_use_four_board_cards() {
        let hole = cards(&["2d", "3c", "7h", "8h"]);
        let board = cards(&["As", "Ks", "Qs", "Js", "Ts"]);
        // Board has royal flush, but Omaha needs exactly 2 hole cards
        let rank = evaluate_omaha(&hole, &board);
        assert_ne!(rank.category, HandCategory::StraightFlush);
    }

    // --- Hand descriptions ---

    #[test]
    fn description_high_card() {
        let rank = eval5([c("As"), c("Kd"), c("9h"), c("7c"), c("2s")]);
        assert_eq!(hand_description(&rank), "ace-high");
    }

    #[test]
    fn description_pair() {
        let rank = eval5([c("5s"), c("5d"), c("Ah"), c("Kc"), c("2s")]);
        assert_eq!(hand_description(&rank), "pair of fives");
    }

    #[test]
    fn description_two_pair() {
        let rank = eval5([c("Ks"), c("Kd"), c("5h"), c("5c"), c("9s")]);
        assert_eq!(hand_description(&rank), "two pair, kings and fives");
    }

    #[test]
    fn description_three_of_a_kind() {
        let rank = eval5([c("7s"), c("7d"), c("7h"), c("Kc"), c("2s")]);
        assert_eq!(hand_description(&rank), "three of a kind, sevens");
    }

    #[test]
    fn description_straight_ace_to_ten() {
        let rank = eval5([c("Ac"), c("Kd"), c("Qh"), c("Js"), c("Tc")]);
        assert_eq!(hand_description(&rank), "straight, ace to ten");
    }

    #[test]
    fn description_straight_ace_to_five() {
        let rank = eval5([c("5s"), c("4d"), c("3h"), c("2c"), c("As")]);
        assert_eq!(hand_description(&rank), "straight, ace to five");
    }

    #[test]
    fn description_straight_normal() {
        let rank = eval5([c("9s"), c("8d"), c("7h"), c("6c"), c("5s")]);
        assert_eq!(hand_description(&rank), "straight, nine to five");
    }

    #[test]
    fn description_flush() {
        let rank = eval5([c("As"), c("Js"), c("9s"), c("5s"), c("3s")]);
        assert_eq!(hand_description(&rank), "flush, ace-high");
    }

    #[test]
    fn description_full_house() {
        let rank = eval5([c("Ks"), c("Kd"), c("Kh"), c("5c"), c("5s")]);
        assert_eq!(hand_description(&rank), "full house, kings full of fives");
    }

    #[test]
    fn description_four_of_a_kind() {
        let rank = eval5([c("Qs"), c("Qd"), c("Qh"), c("Qc"), c("2s")]);
        assert_eq!(hand_description(&rank), "four of a kind, queens");
    }

    #[test]
    fn description_royal_flush() {
        let rank = eval5([c("As"), c("Ks"), c("Qs"), c("Js"), c("Ts")]);
        assert_eq!(hand_description(&rank), "royal flush");
    }

    #[test]
    fn description_straight_flush() {
        let rank = eval5([c("9h"), c("8h"), c("7h"), c("6h"), c("5h")]);
        assert_eq!(hand_description(&rank), "straight flush, nine-high");
    }

    // --- Holding descriptions (contextual) ---

    #[test]
    fn holding_preflop_pocket_pair() {
        let hole = cards(&["As", "Ad"]);
        assert_eq!(holding_description(&hole, &[]), "pocket aces");
    }

    #[test]
    fn holding_preflop_suited() {
        let hole = cards(&["As", "Ks"]);
        assert_eq!(holding_description(&hole, &[]), "A-K suited");
    }

    #[test]
    fn holding_preflop_offsuit() {
        let hole = cards(&["As", "Kd"]);
        assert_eq!(holding_description(&hole, &[]), "A-K offsuit");
    }

    #[test]
    fn holding_top_pair() {
        let hole = cards(&["As", "2d"]);
        let board = cards(&["Ac", "Kh", "9s"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("top pair"), "expected top pair, got: {desc}");
    }

    #[test]
    fn holding_overpair() {
        let hole = cards(&["As", "Ad"]);
        let board = cards(&["Kc", "Qh", "9s"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("overpair"), "expected overpair, got: {desc}");
    }

    #[test]
    fn holding_set_vs_trips() {
        let hole_set = cards(&["7s", "7d"]);
        let board = cards(&["7h", "Kc", "2s"]);
        let desc_set = holding_description(&hole_set, &board);
        assert!(desc_set.contains("set"), "expected set, got: {desc_set}");

        let hole_trips = cards(&["7s", "Kd"]);
        let board_trips = cards(&["7h", "7c", "2s"]);
        let desc_trips = holding_description(&hole_trips, &board_trips);
        assert!(desc_trips.contains("trip"), "expected trips, got: {desc_trips}");
    }

    #[test]
    fn holding_bottom_pair() {
        let hole = cards(&["2s", "3d"]);
        let board = cards(&["Ac", "Kh", "2d"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("bottom pair"), "expected bottom pair, got: {desc}");
    }

    #[test]
    fn holding_partial_cards() {
        let hole = cards(&["As"]);
        let desc = holding_description(&hole, &[]);
        assert_eq!(desc, "As");
    }

    // --- Draw detection ---

    #[test]
    fn detect_flush_draw_with_ace() {
        let hole = cards(&["As", "2s"]);
        let board = cards(&["Ks", "9s", "3d"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("nut flush draw"), "expected nut flush draw, got: {desc}");
    }

    #[test]
    fn detect_flush_draw_non_nut() {
        let hole = cards(&["Qs", "2s"]);
        let board = cards(&["Ks", "9s", "3d"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("flush draw"), "expected flush draw, got: {desc}");
        assert!(!desc.contains("nut"), "should not be nut flush draw, got: {desc}");
    }

    #[test]
    fn detect_oesd() {
        let hole = cards(&["8s", "7d"]);
        let board = cards(&["6c", "5h", "2s"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("open-ended straight draw"), "expected OESD, got: {desc}");
    }

    #[test]
    fn detect_gutshot() {
        let hole = cards(&["8s", "7d"]);
        let board = cards(&["5c", "4h", "2s"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("gutshot straight draw"), "expected gutshot, got: {desc}");
    }

    #[test]
    fn no_draw_on_river() {
        let hole = cards(&["8s", "7d"]);
        let board = cards(&["6c", "5h", "2s", "Kd", "Qh"]);
        let desc = holding_description(&hole, &board);
        assert!(!desc.contains("draw"), "no draws on river, got: {desc}");
    }

    #[test]
    fn board_only_straight_draw_not_reported() {
        // Board has 5-6-7, hole cards are Ac Kd — no hole card contributes to the draw.
        let hole = cards(&["Ac", "Kd"]);
        let board = cards(&["5c", "6h", "7d"]);
        let desc = holding_description(&hole, &board);
        assert!(
            !desc.contains("straight draw"),
            "board-only straight draw should not be reported, got: {desc}"
        );
    }

    #[test]
    fn board_only_flush_draw_not_reported() {
        // Board has 3 spades, hole cards have no spades.
        let hole = cards(&["Ac", "Kd"]);
        let board = cards(&["2s", "5s", "9s"]);
        let desc = holding_description(&hole, &board);
        assert!(
            !desc.contains("flush draw"),
            "board-only flush draw should not be reported, got: {desc}"
        );
    }

    #[test]
    fn hole_contributing_straight_draw_reported() {
        // Board has 5-6, hole has 8-7 → 5-6-7-8 = OESD (hole contributes)
        let hole = cards(&["8s", "7d"]);
        let board = cards(&["6c", "5h", "2s"]);
        let desc = holding_description(&hole, &board);
        assert!(desc.contains("straight draw"), "expected straight draw, got: {desc}");
    }

    // --- HandRank::worst ---

    #[test]
    fn worst_rank_loses_to_everything() {
        let worst = HandRank::worst();
        let high_card = eval5([c("7s"), c("5d"), c("4h"), c("3c"), c("2s")]);
        assert!(worst < high_card);
    }

    // --- rank_name / rank_name_plural ---

    #[test]
    fn rank_names() {
        assert_eq!(rank_name(14), "ace");
        assert_eq!(rank_name(2), "two");
        assert_eq!(rank_name(0), "unknown");
        assert_eq!(rank_name_plural(14), "aces");
        assert_eq!(rank_name_plural(6), "sixes");
    }
}
