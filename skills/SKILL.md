---
description: 'Analyze PokerNow hand histories using poker-cli. USE FOR: - Computing player stats (VPIP, PFR, 3-bet, C-bet, AF, WTSD) - Replaying specific hands with made-hand descriptions - Searching/filtering hands by player, pot size, showdown - Comparing player performance across sessions TRIGGERS: - "poker stats", "hand history", "analyze session" - "show hand", "replay hand", "what happened in hand" - "search hands", "big pots", "showdown hands" - "player stats", "VPIP", "how did X play"'
allowed-tools:
  - Bash(./target/release/poker-cli:*)
  - Bash(cargo build:*)
---

## Context
- Binary: `./target/release/poker-cli` (build with `cargo build --release` if missing)
- Hand history files: PokerNow JSON exports, typically at `~/dev/pokernow/hands/*.json`

## Commands

### Player stats
```bash
./target/release/poker-cli stats <file1.json> [file2.json ...]
```
Output: ranked player list with VPIP, PFR, 3-Bet%, Fold-to-3B, C-Bet%, AF, WTSD, W$SD, positional breakdowns, all-in EV diff. Multi-file merges stats across sessions.

### Hand replay
```bash
./target/release/poker-cli hand <hand-id> <file.json>
```
Shows full hand: players, stacks, positions, street-by-street actions, board cards, made-hand descriptions (top pair, flush draw, set, etc.), and results. Hand IDs are opaque strings from the JSON (e.g. `nyamg3i3yuit`). On invalid ID, all available IDs are listed.

### Search/filter hands
```bash
./target/release/poker-cli search [flags] <file.json>
```
Flags:
- `--player <name>` — player VPIP'd in the hand
- `--saw-flop <name>` / `--saw-turn` / `--saw-river` — player reached that street
- `--min-pot <bb>` / `--max-pot <bb>` — pot size bounds in big blinds
- `--showdown` / `--no-showdown` — filter by showdown
- `--sort pot` — sort results by pot size descending (default: hand number)

### Player unification
```bash
./target/release/poker-cli --unify-players "Name1,Name2;Name3,Name4" stats <file.json>
```
Merges player identities. First name in each semicolon-separated group is canonical. Use when same person has multiple PokerNow accounts.

## Workflow patterns

**Session review**: Run `stats` first to get the overview, then `search --sort pot` to find interesting hands, then `hand <id>` to replay them.

**Player investigation**: Use `search --player X --min-pot 50` to find hands a player was involved in, then replay specific hands.

**Cross-session comparison**: Pass multiple JSON files to `stats` to aggregate across sessions. Use `--unify-players` if players changed names/accounts between sessions.

## Notes
- Pot sizes in search results are in big blinds
- Hand descriptions are contextual for Hold'em (overpair, top pair, flush draw) and standard for Omaha
- Player name matching is case-insensitive
- Bomb pot hands are excluded from preflop stats (VPIP, PFR) but included in P&L
