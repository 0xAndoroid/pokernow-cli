# poker-cli

Fast CLI for analyzing [PokerNow](https://www.pokernow.club/) hand history JSON exports. Computes HUD-style stats, replays individual hands with made-hand descriptions, and searches/filters hands by criteria.

Only standard Texas Hold'em hands are processed. Omaha, bomb pots, and double board / run-it-twice hands are silently filtered out.

## Build

```
cargo build --release
```

Requires Rust 2024 edition (1.85+).

## Usage

### Stats

Compute per-player HUD statistics ranked by P&L:

```
poker-cli stats session1.json session2.json
```

Output includes VPIP, PFR, 3-Bet%, Fold-to-3B, C-Bet%, AF, WTSD, W$SD, positional breakdowns, and all-in EV diff.

### Hand replay

Display a single hand with board, actions, and made-hand descriptions:

```
poker-cli hand <hand-id> session.json
poker-cli hand 43 session.json
```

Accepts either a PokerNow hash ID (e.g. `nyamg3i3yuit`) or a sequential number (1-based index into loaded hands).

### Search

Filter hands by player involvement, pot size, and showdown status:

```
poker-cli search --player Andrew --min-pot 100 --showdown session.json
poker-cli search --saw-flop Andrew --sort pot session.json
```

Flags:
- `--player <name>` — player VPIP'd (voluntarily put money in)
- `--saw-flop <name>` / `--saw-turn` / `--saw-river` — player reached street
- `--min-pot <bb>` / `--max-pot <bb>` — pot size in big blinds
- `--showdown` / `--no-showdown` — showdown filter
- `--sort pot` — sort by pot size (default: hand number)

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

## Architecture

```
src/
  main.rs     CLI entry point (clap), config loading, file resolution
  config.rs   config.toml parsing, tilde expansion
  parser.rs   JSON deserialization, event processing, position assignment
  card.rs     Card representation, 5-card hand evaluation, hand descriptions
  stats.rs    HUD stat computation (VPIP, PFR, 3-bet, C-bet, AF, WTSD, EV)
  display.rs  Hand replay formatting
  search.rs   Hand filtering and search output
```

## Known limitations

- Only Texas Hold'em hands are supported
- Omaha, bomb pots, and double board / run-it-twice are filtered out (future work)
- No automated test suite — validate against real PokerNow exports

## Input format

See [CLAUDE.md](CLAUDE.md) for the complete PokerNow JSON format reference.
