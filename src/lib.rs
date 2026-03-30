pub mod card;
pub mod config;
pub mod display;
pub mod parser;
pub mod parser_log;
pub mod ranking;
pub mod search;
pub mod stats;
pub mod summary;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TableSize {
    HeadsUp,
    Short,
    Full,
}

impl TableSize {
    pub fn from_player_count(n: usize) -> Self {
        match n {
            0..=2 => Self::HeadsUp,
            3..=6 => Self::Short,
            _ => Self::Full,
        }
    }
}

pub fn format_chips(amount: f64) -> String {
    if amount.fract() == 0.0 { format!("{}", amount as i64) } else { format!("{amount}") }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn table_size_heads_up() {
        assert_eq!(TableSize::from_player_count(2), TableSize::HeadsUp);
        assert_eq!(TableSize::from_player_count(1), TableSize::HeadsUp);
        assert_eq!(TableSize::from_player_count(0), TableSize::HeadsUp);
    }

    #[test]
    fn table_size_short() {
        assert_eq!(TableSize::from_player_count(3), TableSize::Short);
        assert_eq!(TableSize::from_player_count(4), TableSize::Short);
        assert_eq!(TableSize::from_player_count(5), TableSize::Short);
        assert_eq!(TableSize::from_player_count(6), TableSize::Short);
    }

    #[test]
    fn table_size_full() {
        assert_eq!(TableSize::from_player_count(7), TableSize::Full);
        assert_eq!(TableSize::from_player_count(8), TableSize::Full);
        assert_eq!(TableSize::from_player_count(9), TableSize::Full);
        assert_eq!(TableSize::from_player_count(10), TableSize::Full);
    }

    #[test]
    fn table_size_filter_default_excludes_hu() {
        let sizes: HashSet<TableSize> = [TableSize::Short, TableSize::Full].into();
        assert!(!sizes.contains(&TableSize::from_player_count(2)));
        assert!(sizes.contains(&TableSize::from_player_count(3)));
        assert!(sizes.contains(&TableSize::from_player_count(7)));
    }

    #[test]
    fn table_size_filter_hu_only() {
        let sizes: HashSet<TableSize> = [TableSize::HeadsUp].into();
        assert!(sizes.contains(&TableSize::from_player_count(2)));
        assert!(!sizes.contains(&TableSize::from_player_count(3)));
        assert!(!sizes.contains(&TableSize::from_player_count(7)));
    }

    #[test]
    fn table_size_filter_all() {
        let sizes: HashSet<TableSize> =
            [TableSize::HeadsUp, TableSize::Short, TableSize::Full].into();
        assert!(sizes.contains(&TableSize::from_player_count(2)));
        assert!(sizes.contains(&TableSize::from_player_count(5)));
        assert!(sizes.contains(&TableSize::from_player_count(9)));
    }

    #[test]
    fn filter_hands_by_table_size() {
        use crate::parser::test_helpers::*;

        let hu = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let short = HandBuilder::new()
            .number(2)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(1)
            .fold(2)
            .win(3, 1.5);

        let mut data = parse_multi_game_data(&[&hu, &short]);
        assert_eq!(data.hands.len(), 2);

        let sizes: HashSet<TableSize> = [TableSize::Short, TableSize::Full].into();
        data.hands.retain(|h| sizes.contains(&TableSize::from_player_count(h.players.len())));
        assert_eq!(data.hands.len(), 1);
        assert_eq!(data.hands[0].number, 2);
    }

    #[test]
    fn filter_hands_hu_only() {
        use crate::parser::test_helpers::*;

        let hu = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let short = HandBuilder::new()
            .number(2)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(1)
            .fold(2)
            .win(3, 1.5);

        let mut data = parse_multi_game_data(&[&hu, &short]);
        let sizes: HashSet<TableSize> = [TableSize::HeadsUp].into();
        data.hands.retain(|h| sizes.contains(&TableSize::from_player_count(h.players.len())));
        assert_eq!(data.hands.len(), 1);
        assert_eq!(data.hands[0].number, 1);
    }

    #[test]
    fn filter_hands_all_sizes() {
        use crate::parser::test_helpers::*;

        let hu = HandBuilder::new()
            .number(1)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .dealer(1)
            .sb(1, 0.5)
            .bb(2, 1.0)
            .fold(1)
            .win(2, 1.5);

        let short = HandBuilder::new()
            .number(2)
            .player("p1", 1, "Alice", 100.0)
            .player("p2", 2, "Bob", 100.0)
            .player("p3", 3, "Charlie", 100.0)
            .dealer(1)
            .sb(2, 0.5)
            .bb(3, 1.0)
            .fold(1)
            .fold(2)
            .win(3, 1.5);

        let mut data = parse_multi_game_data(&[&hu, &short]);
        let sizes: HashSet<TableSize> =
            [TableSize::HeadsUp, TableSize::Short, TableSize::Full].into();
        data.hands.retain(|h| sizes.contains(&TableSize::from_player_count(h.players.len())));
        assert_eq!(data.hands.len(), 2);
    }
}
