mod card;
mod display;
mod parser;
mod search;
mod stats;

use std::collections::HashMap;
use std::process;

use clap::{Parser, Subcommand};

use search::{SearchFilter, SortField};

#[derive(Parser)]
#[command(name = "poker-cli", about = "PokerNow hand history analyzer")]
struct Cli {
    /// Merge player identities (format: "name1,name2;name3,name4")
    #[arg(long)]
    unify_players: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args)]
struct FileArgs {
    /// Hand history JSON files
    #[arg(required = true)]
    files: Vec<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Show player statistics
    Stats {
        #[command(flatten)]
        files: FileArgs,
    },
    /// Display a specific hand
    Hand {
        /// Hand ID to display
        id: String,
        #[command(flatten)]
        files: FileArgs,
    },
    /// Search/filter hands
    Search {
        #[command(flatten)]
        files: FileArgs,
        #[arg(long)]
        player: Option<String>,
        #[arg(long)]
        saw_flop: Option<String>,
        #[arg(long)]
        saw_turn: Option<String>,
        #[arg(long)]
        saw_river: Option<String>,
        #[arg(long)]
        min_pot: Option<f64>,
        #[arg(long)]
        max_pot: Option<f64>,
        #[arg(long)]
        showdown: bool,
        #[arg(long)]
        no_showdown: bool,
        #[arg(long, default_value = "hand_id")]
        sort: String,
    },
}

fn build_unify_map(spec: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for group in spec.split(';') {
        let names: Vec<&str> = group.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if names.len() < 2 {
            continue;
        }
        let canonical = names[0].to_string();
        for &name in &names[1..] {
            map.insert(name.to_string(), canonical.clone());
        }
    }
    map
}

fn parse_sort_field(s: &str) -> SortField {
    match s {
        "pot" => SortField::Pot,
        _ => SortField::HandId,
    }
}

enum CliAction {
    Stats,
    Hand(String),
    Search(SearchFilter),
}

fn main() {
    let cli = Cli::parse();

    let unify_map = cli.unify_players.as_deref().map(build_unify_map).unwrap_or_default();

    let (files, action) = match cli.command {
        Command::Stats { files } => (files.files, CliAction::Stats),
        Command::Hand { id, files } => (files.files, CliAction::Hand(id)),
        Command::Search {
            files,
            player,
            saw_flop,
            saw_turn,
            saw_river,
            min_pot,
            max_pot,
            showdown,
            no_showdown,
            sort,
        } => {
            let showdown_filter = match (showdown, no_showdown) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            };
            let filter = SearchFilter {
                player,
                saw_flop,
                saw_turn,
                saw_river,
                min_pot,
                max_pot,
                showdown: showdown_filter,
                sort: parse_sort_field(&sort),
            };
            (files.files, CliAction::Search(filter))
        }
    };

    let data = match parser::parse_files(&files, &unify_map) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to parse hand history: {e}");
            process::exit(1);
        }
    };

    match action {
        CliAction::Stats => {
            let result = stats::compute_stats(&data);
            stats::print_stats(&result);
        }
        CliAction::Hand(id) => {
            if let Some(h) = data.hands.iter().find(|h| h.id == id) {
                display::display_hand(h);
            } else {
                eprintln!("Hand '{id}' not found");
                let ids: Vec<&str> = data.hands.iter().map(|h| h.id.as_str()).collect();
                eprintln!("Available hand IDs: {}", ids.join(", "));
                process::exit(1);
            }
        }
        CliAction::Search(filter) => {
            let results = search::search_hands(&data, &filter);
            search::print_results(&results);
        }
    }
}
