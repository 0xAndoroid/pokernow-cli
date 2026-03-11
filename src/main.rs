use std::collections::HashMap;
use std::path::Path;
use std::process;

use clap::{Parser, Subcommand};

use poker_cli::config::Config;
use poker_cli::display;
use poker_cli::parser;
use poker_cli::search::{self, SearchFilter, SortField};
use poker_cli::stats;

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
        /// Hand ID or sequential number (1, 2, 3...)
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

fn resolve_files_and_unify(
    cli_files: Vec<String>,
    cli_unify: Option<String>,
) -> (Vec<String>, HashMap<String, String>) {
    let config = load_config();

    let files = if cli_files.is_empty() {
        config.as_ref().map_or_else(Vec::new, Config::expanded_files)
    } else {
        cli_files
    };

    let unify = if let Some(spec) = cli_unify {
        build_unify_map(&spec)
    } else {
        config.as_ref().map_or_else(HashMap::new, Config::unify_map)
    };

    (files, unify)
}

fn load_config() -> Option<Config> {
    let path = Path::new("config.toml");
    if !path.exists() {
        return None;
    }
    match Config::load(path) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("Warning: failed to parse config.toml: {e}");
            None
        }
    }
}

fn main() {
    let cli = Cli::parse();

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

    let (files, unify_map) = resolve_files_and_unify(files, cli.unify_players);

    if files.is_empty() {
        eprintln!("Error: no hand history files specified.\n");
        eprintln!("Provide files as arguments:");
        eprintln!("  poker-cli stats game1.json game2.json\n");
        eprintln!("Or create a config.toml in the current directory:");
        eprintln!("  files = [\"~/dev/pokernow/hands/session.json\"]");
        process::exit(1);
    }

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
            let hand = if let Ok(n) = id.parse::<usize>() {
                if n == 0 { None } else { data.hands.get(n - 1) }
            } else {
                data.hands.iter().find(|h| h.id == id)
            };

            if let Some(h) = hand {
                display::display_hand(h);
            } else {
                eprintln!("Hand '{id}' not found");
                eprintln!("Available: hands 1-{} (or use hash ID)", data.hands.len());
                process::exit(1);
            }
        }
        CliAction::Search(filter) => {
            let results = search::search_hands(&data, &filter);
            search::print_results(&results);
        }
    }
}
