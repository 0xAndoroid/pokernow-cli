pub mod card;
pub mod config;
pub mod display;
pub mod parser;
pub mod search;
pub mod stats;
pub mod summary;

pub fn format_chips(amount: f64) -> String {
    if amount.fract() == 0.0 { format!("{}", amount as i64) } else { format!("{amount}") }
}
