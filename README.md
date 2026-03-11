# poker-cli

Fast CLI for analyzing [PokerNow](https://www.pokernow.club/) hand history JSON exports. Computes HUD-style stats, replays individual hands with made-hand descriptions, and searches/filters hands by criteria.

## Build

```
cargo build --release
```

Requires Rust 2024 edition (1.85+).

## Usage

All commands take one or more PokerNow JSON hand history files as trailing arguments.

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
```

Hand IDs are the opaque strings from the PokerNow JSON (e.g. `nyamg3i3yuit`). If you don't know the ID, the error message lists all available IDs.

Hold'em hands show contextual descriptions (top pair, overpair, set, flush draw, etc.). Omaha hands use standard hand names via proper 2+3 evaluation.

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

Merge multiple PokerNow player identities into one (e.g. same person with different accounts):

```
poker-cli --unify-players "Andrew,aryan;Steve,steveooooo" stats session.json
```

The first name in each group becomes the canonical identity. Semicolons separate groups.

## Architecture

```
src/
  main.rs     CLI entry point (clap)
  parser.rs   JSON deserialization, event processing, position assignment
  card.rs     Card representation, 5-card hand evaluation, Omaha evaluation, hand descriptions
  stats.rs    HUD stat computation (VPIP, PFR, 3-bet, C-bet, AF, WTSD, EV)
  display.rs  Hand replay formatting
  search.rs   Hand filtering and search output
```

### Key design decisions

- **Chip amounts are `f64`** — PokerNow uses float values natively; no cent conversion needed for play-money games.
- **Hand evaluation is brute-force C(n,5)** — fast enough for 7-card Hold'em and 9-card Omaha; no lookup tables needed.
- **Omaha uses strict 2+3 rule** — `evaluate_omaha` enumerates C(4,2) x C(board,3) combinations.
- **Antes summed separately in net profit** — antes are additive on top of blind/bet amounts, which use per-street max tracking.
- **Spurious fold filtering** — PokerNow emits phantom fold events; these are detected by checking for later actions (check/call/bet/win) by the same seat, excluding SHOW events (a player can fold then show cards).
- **Showdown detection** — type 15 fires every hand; real showdowns require 2+ type 12 SHOW events.

## Input format

See [POKERNOW.md](POKERNOW.md) for the complete PokerNow JSON hand history format reference.
