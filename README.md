# poker-cli

Fast CLI for analyzing [PokerNow](https://www.pokernow.club/) hand history JSON exports. Computes HUD-style stats, replays individual hands with made-hand descriptions, and searches/filters hands by criteria.

Only standard Texas Hold'em hands are processed. Omaha, bomb pots, and double-board game hands are silently filtered out. Run-it-twice hands are fully supported — the first run's board is used for evaluation and stats, while the hand replay displays both runs with their results.

## Installation

Requires Rust 1.85+ (2024 edition).

Install from source:

```bash
git clone https://github.com/andoroid/poker-cli.git
cd poker-cli
cargo install --path ./
```

### Getting hand histories

1. Go to your [PokerNow](https://www.pokernow.club/) game
2. Click the hamburger menu (top-right) → **Download Hand History**
3. Save the JSON file

## Usage

### Stats

Compute per-player HUD statistics ranked by P&L:

```
poker-cli stats session1.json session2.json
poker-cli stats --player Andrew session.json    # single-player compact view
```

Output includes VPIP, PFR, 3-Bet%, Fold-to-3B, C-Bet%, Fold-to-CB%, AF, WTSD, W$SD, WWSF, positional breakdowns, and all-in EV diff. The `--player` flag shows stats for one player only.

### Hand replay

Display a single hand with board, actions, and made-hand descriptions:

```
poker-cli hand <hand-id> session.json
poker-cli hand 245 session.json                # by hand number (from search output)
poker-cli hand geyaotgpt14p session.json       # by PokerNow hash ID
```

Accepts either a PokerNow hash ID or a hand number (matching the `Hand #` column in search output). The header shows hand number, hash ID, stakes, player count, and effective stack in BB. The replay ends with per-player net P&L.

### Search

Filter hands by player involvement, pot size, showdown, and P&L:

```
poker-cli search --player Andrew --min-pot 100 --showdown session.json
poker-cli search --player Andrew --lost --sort pot session.json
poker-cli search --saw-flop Andrew --sort pot session.json
```

Flags:
- `--player <name>` — hands where player was dealt in
- `--saw-flop <name>` / `--saw-turn` / `--saw-river` — player reached street
- `--min-pot <bb>` / `--max-pot <bb>` — pot size in big blinds
- `--showdown` / `--no-showdown` — showdown filter (player-aware when combined with `--player`)
- `--won` — only hands where `--player` won money
- `--lost` — only hands where `--player` lost money
- `--sort pot` — sort by pot size (default: hand number)

Output includes hand number, PokerNow hash ID, pot size, showdown status, winner, and amount. When `--player` is specified, a "Player Net" column shows the player's profit/loss per hand.

### Summary

Compact one-screen session overview:

```
poker-cli summary session.json
```

Shows hand count, stakes, player count, biggest pot, and a P&L table with VPIP/PFR/BB-per-hand for all players ranked by profit.

### Player unification

Merge multiple PokerNow player identities into one:

```
poker-cli --unify-players "Andrew,aryan;Steve,steveooooo" stats session.json
```

The first name in each group becomes the canonical identity. Semicolons separate groups.

## Config file

Create `config.toml` in the working directory to set defaults:

```toml
# Default files when none given on CLI (supports ~ expansion)
files = [
  "~/dev/pokernow/hands/2026-03-11.json",
  "~/dev/pokernow/hands/2026-03-10.json",
]

# Player unification (same as --unify-players but persistent)
[unify]
pranav = ["pranav", "pranavv"]
andrew = ["Andrew", "aryan"]
```

CLI arguments override config values. If files are given on the command line, `config.toml` files are ignored. If `--unify-players` is passed, the config `[unify]` section is ignored.

Use `--no-config` to skip loading `config.toml` entirely. When config files are loaded, a "Loaded N file(s) from config.toml" message is printed to stderr.

## Architecture

```
src/
  main.rs     CLI entry point (clap), config loading, file resolution
  config.rs   config.toml parsing, tilde expansion
  parser.rs   JSON deserialization, event processing, position assignment
  card.rs     Card representation, 5-card hand evaluation, hand descriptions
  stats.rs    HUD stat computation (VPIP, PFR, 3-bet, C-bet, AF, WTSD, W$SD, WWSF, EV)
  display.rs  Hand replay formatting, net P&L, effective stacks
  search.rs   Hand filtering, player-aware showdown, won/lost filters
  summary.rs  Compact session summary
```

## Known limitations

- Only Texas Hold'em hands are supported
- Omaha, bomb pots, and double-board games are filtered out
- Run-it-twice hands are supported (first run for stats, both runs displayed)
- 146 tests with ~92% line coverage. Run `cargo test` and `cargo llvm-cov`

## Input format

See [CLAUDE.md](CLAUDE.md) for the complete PokerNow JSON format reference.
