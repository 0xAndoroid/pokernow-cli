use std::collections::HashMap;
use std::path::Path;
use std::process;

use clap::{Parser, Subcommand, ValueEnum};

use poker_cli::config::{BlindRemap, Config};
use poker_cli::display;
use poker_cli::parser::{self, GameData};
use poker_cli::parser_log;
use poker_cli::search::{self, SearchFilter, SortField};
use poker_cli::stats;
use poker_cli::summary;

#[derive(Clone, Copy, ValueEnum)]
enum LogFormat {
    Json,
    Log,
}

#[derive(Parser)]
#[command(
    name = "poker-cli",
    about = "PokerNow hand history analyzer",
    after_help = "Config: place a config.toml in the working directory to set default files and \
                  player unification rules. Use --no-config to disable."
)]
struct Cli {
    /// Merge player identities (format: "name1,name2;name3,name4")
    #[arg(long)]
    unify_players: Option<String>,

    /// Disable loading config.toml
    #[arg(long)]
    no_config: bool,

    /// Force file format (auto-detects from extension if omitted)
    #[arg(long)]
    log_format: Option<LogFormat>,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args)]
struct FileArgs {
    /// Hand history files (.json or .log/.txt)
    files: Vec<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Show player statistics
    Stats {
        #[command(flatten)]
        files: FileArgs,
        /// Show stats for a single player only
        #[arg(long)]
        player: Option<String>,
    },
    /// Display a specific hand (by hand number or hash ID)
    Hand {
        /// Hand number (from search output) or PokerNow hash ID
        id: String,
        #[command(flatten)]
        files: FileArgs,
    },
    /// Search and filter hands
    Search {
        #[command(flatten)]
        files: FileArgs,
        /// Filter to hands where this player was dealt in
        #[arg(long)]
        player: Option<String>,
        /// Filter to hands where this player saw the flop
        #[arg(long)]
        saw_flop: Option<String>,
        /// Filter to hands where this player saw the turn
        #[arg(long)]
        saw_turn: Option<String>,
        /// Filter to hands where this player saw the river
        #[arg(long)]
        saw_river: Option<String>,
        /// Minimum pot size in BB
        #[arg(long)]
        min_pot: Option<f64>,
        /// Maximum pot size in BB
        #[arg(long)]
        max_pot: Option<f64>,
        /// Only hands that went to showdown (player-aware with --player)
        #[arg(long)]
        showdown: bool,
        /// Only hands that did NOT go to showdown
        #[arg(long)]
        no_showdown: bool,
        /// Only hands where --player won money (requires --player)
        #[arg(long)]
        won: bool,
        /// Only hands where --player lost money (requires --player)
        #[arg(long)]
        lost: bool,
        /// Sort results: "hand_id" (default) or "pot"
        #[arg(long, default_value = "hand_id")]
        sort: String,
    },
    /// Show a compact session summary
    Summary {
        #[command(flatten)]
        files: FileArgs,
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
    Stats(Option<String>),
    Hand(String),
    Search(SearchFilter),
    Summary,
}

fn resolve_files_and_unify(
    cli_files: Vec<String>,
    cli_unify: Option<String>,
    no_config: bool,
) -> (Vec<String>, HashMap<String, String>, Vec<BlindRemap>, bool) {
    let config = if no_config { None } else { load_config() };
    let from_config = config.is_some() && cli_files.is_empty();

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

    let blind_remap = config.as_ref().map_or_else(Vec::new, |c| c.blind_remap.clone());

    (files, unify, blind_remap, from_config)
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

fn is_log_file(path: &str) -> bool {
    let p = Path::new(path);
    matches!(p.extension().and_then(|e| e.to_str()), Some("log" | "txt"))
}

fn parse_all_files(
    files: &[String],
    format_override: Option<LogFormat>,
    unify_map: &HashMap<String, String>,
    blind_remap: &[BlindRemap],
) -> Result<GameData, Box<dyn std::error::Error>> {
    let (json_files, log_files): (Vec<_>, Vec<_>) = match format_override {
        Some(LogFormat::Json) => {
            for f in files {
                if is_log_file(f) {
                    eprintln!("Warning: skipping {f} (not JSON format)");
                }
            }
            (files.iter().filter(|f| !is_log_file(f)).cloned().collect(), vec![])
        }
        Some(LogFormat::Log) => {
            for f in files {
                if !is_log_file(f) {
                    eprintln!("Warning: skipping {f} (not log format)");
                }
            }
            (vec![], files.iter().filter(|f| is_log_file(f)).cloned().collect())
        }
        None => files.iter().cloned().partition(|f| !is_log_file(f)),
    };

    let mut data = if json_files.is_empty() {
        GameData {
            hands: Vec::new(),
            player_names: HashMap::new(),
        }
    } else {
        parser::parse_files(&json_files, unify_map, blind_remap)?
    };

    if !log_files.is_empty() {
        let log_data = parser_log::parse_log_files(&log_files, unify_map, blind_remap)?;
        let offset = data.hands.len() as u32;
        for mut hand in log_data.hands {
            hand.number += offset;
            data.hands.push(hand);
        }
        data.player_names.extend(log_data.player_names);
    }

    Ok(data)
}

fn main() {
    let cli = Cli::parse();
    let no_config = cli.no_config;
    let log_format = cli.log_format;

    let (files, action) = match cli.command {
        Command::Stats { files, player } => (files.files, CliAction::Stats(player)),
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
            won,
            lost,
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
                won,
                lost,
                sort: parse_sort_field(&sort),
            };
            (files.files, CliAction::Search(filter))
        }
        Command::Summary { files } => (files.files, CliAction::Summary),
    };

    let (files, unify_map, blind_remap, from_config) =
        resolve_files_and_unify(files, cli.unify_players, no_config);

    if files.is_empty() {
        eprintln!("Error: no hand history files specified.\n");
        eprintln!("Provide files as arguments:");
        eprintln!("  poker-cli stats game1.json game2.json\n");
        eprintln!("Or create a config.toml in the current directory:");
        eprintln!("  files = [\"~/dev/pokernow/hands/session.json\"]");
        process::exit(1);
    }

    if from_config {
        eprintln!("Loaded {} file(s) from config.toml", files.len());
    }

    let data = match parse_all_files(&files, log_format, &unify_map, &blind_remap) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to parse hand history: {e}");
            process::exit(1);
        }
    };

    match action {
        CliAction::Stats(player_filter) => {
            let result = stats::compute_stats(&data);
            if let Some(ref name) = player_filter {
                stats::print_single_player_stats(&result, name);
            } else {
                stats::print_stats(&result);
            }
        }
        CliAction::Hand(id) => {
            let hand = if let Ok(n) = id.parse::<u32>() {
                data.hands
                    .iter()
                    .find(|h| h.number == n)
                    .or_else(|| if n == 0 { None } else { data.hands.get((n - 1) as usize) })
            } else {
                data.hands.iter().find(|h| h.id == id)
            };

            if let Some(h) = hand {
                display::display_hand(h);
            } else {
                eprintln!("Hand '{id}' not found.");
                eprintln!(
                    "{} hands loaded (#{}-#{}).",
                    data.hands.len(),
                    data.hands.first().map_or(0, |h| h.number),
                    data.hands.last().map_or(0, |h| h.number),
                );
                eprintln!("Use `search` to find hands by player, pot size, or showdown.");
                process::exit(1);
            }
        }
        CliAction::Search(filter) => {
            let results = search::search_hands(&data, &filter);
            search::print_results(&results);
        }
        CliAction::Summary => {
            summary::print_summary(&data);
        }
    }
}
