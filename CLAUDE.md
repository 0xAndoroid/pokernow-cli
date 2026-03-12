# poker-cli

PokerNow hand history analyzer. Rust CLI that parses JSON exports from [pokernow.club](https://www.pokernow.club/).

## Build & test

```bash
cargo build --release
cargo test              # 146 tests (134 unit + 12 integration)
cargo clippy            # pedantic lints enabled — must be warning-free
cargo fmt -- --check    # must pass
cargo llvm-cov --summary-only  # coverage report (~92% line coverage)
```

Integration tests use fixture data in `tests/fixtures/` (no external data needed).

## Architecture

```
src/main.rs      CLI (clap). Parses args, dispatches to stats/display/search/summary.
                 Config loading, file resolution, --no-config flag.
src/config.rs    config.toml parsing (files, player unification). Tilde expansion.
src/parser.rs    JSON deserialization → Hand/Action/Winner structs. Position assignment.
                 Spurious fold filtering. Net profit calculation. is_monetary() lives here.
                 Filters: only Hold'em, no bomb pots, no double-board games.
                 Run-it-twice hands supported (uses first run for eval, displays both).
src/card.rs      Card(u8) packed repr. 5-card evaluator (brute-force C(n,5)).
                 evaluate() for Hold'em, evaluate_omaha() for Omaha (2+3 rule).
                 holding_description() — contextual hand descriptions with draw detection.
src/stats.rs     HUD stat computation: VPIP, PFR, 3-bet, C-bet, AF, WTSD, W$SD, WWSF, EV.
                 Single-player view via print_single_player_stats().
src/display.rs   Hand replay output. Run-it-twice shows both runs with results.
                 Per-player net P&L. Effective stacks in header. Hand hash ID in header.
src/search.rs    Hand filtering by player/pot/street/showdown/won/lost.
                 Player-aware showdown filter. Player net column. Hand ID in output.
src/summary.rs   Compact session summary: hand count, P&L table, biggest pot.
```

## Config file

`config.toml` in the working directory. CLI args override config values. Use `--no-config` to disable loading.

```toml
files = [
  "~/dev/pokernow/hands/2026-03-11.json",
  "~/dev/pokernow/hands/2026-03-10.json",
]

[unify]
pranav = ["pranav", "pranavv"]
andrew = ["Andrew", "aryan"]
```

- `files`: default hand history files when none given on CLI. Supports `~` expansion.
- `[unify]`: player unification. Key = canonical name, value = list of aliases.

When config is loaded, prints "Loaded N file(s) from config.toml" to stderr.

## Key conventions

- `Action.kind` not `action_type` (clippy struct_field_names)
- Chip amounts are `f64` (PokerNow native format)
- Card = packed `u8`: `(rank * 4) + suit`. Rank 2-14, suit 0-3
- Stat opportunity fields use `_opp` suffix (e.g. `three_bet_opp`, `cbet_opp`)
- Position enum: `EP, MP, CO, BTN, SB, BB` (uppercase, allowed via clippy)
- No `unwrap()` on user data — use `let...else`, `map_or`, `unwrap_or`

## Known limitations

- **Omaha hands filtered out** — only Texas Hold'em (`gameType: "th"`) is processed
- **Bomb pots filtered out** — `bombPot: true` hands are skipped
- **Double-board games filtered out** — hands with `run > 1` boards but no type 14 (RIT vote)
- **Run-it-twice supported** — first run's board used for eval/stats; display shows both runs
- **`hand` numeric lookup** — matches `hand.number` (JSON number field) first, falls back to array index

## Gotchas

- **Antes are additive**: In `net_profit()`, antes sum separately from blind/bet maxes per street
- **Spurious folds**: ~55% of PokerNow hands have phantom fold events. `remove_spurious_folds()` detects seats that fold then act later (check/call/bet/win), but NOT show (type 12) — folding then showing is legitimate
- **Partial hole cards**: Some SHOW events have `None` values or single cards — `holding_description` guards against <2 cards
- **Type 15 SHOWDOWN fires every hand**: Real showdown requires 2+ type 12 SHOW events (`real_showdown` field)
- **Bet values are cumulative per street**: Type 8 value=10 means 10 total on that street, not 10 on top of previous bet
- **`--showdown` is player-aware**: When combined with `--player`, only returns hands where that player went to showdown (checked via shown_cards or winner cards), not hands where they folded and others showed down
- **`--player` filter uses `player_in_hand`**: Matches any hand where the player was dealt in (not just VPIP'd)

---

## PokerNow JSON format reference

### Top-level structure

```json
{
  "generatedAt": "2026-02-24T17:23:11.830Z",
  "playerId": "wBcyK_YnY6",
  "gameId": "pglF1LvFjtObLvgZ5ctjDDMX-",
  "hands": [...]
}
```

### Hand object

```json
{
  "id": "x75f0vosvwtl",
  "number": "1",
  "gameType": "th",              // "th" = Hold'em, "omaha" = Omaha
  "smallBlind": 1,
  "bigBlind": 1,
  "ante": null,
  "dealerSeat": 3,
  "bombPot": false,
  "players": [...],
  "events": [...]
}
```

### Player object

```json
{
  "id": "wBcyK_YnY6",
  "seat": 1,
  "name": "Andrew",
  "stack": 100,
  "hand": ["7s", "9c"]   // only for exporter + shown hands
}
```

Card format: rank + suit. Rank = {2-9, T, J, Q, K, A}, Suit = {s, h, d, c}. Omaha has 4 cards.

### Event types

| Type | Name | Key Fields |
|------|------|-----------|
| 0 | Fold | seat |
| 1 | Check | seat |
| 2 | Big Blind | seat, value |
| 3 | Small Blind | seat, value |
| 4 | Ante | seat, value |
| 5 | Straddle | seat, value |
| 6 | Dead Blind | seat, value |
| 7 | Call | seat, value, allIn? |
| 8 | Bet/Raise | seat, value, allIn? |
| 9 | Board | turn(1-3), run(1-2), cards |
| 10 | Win | seat, value, pot, cards?, handDescription? |
| 11 | Action Marker | seat (ignore) |
| 12 | Show Cards | seat, cards |
| 14 | Run-It-Twice Vote | approved, approvedSeats |
| 15 | Showdown | (none — fires every hand) |
| 16 | Uncalled Return | seat, value |

### Critical parsing rules

1. **Type 8 value is total, not increment** — raise from 3 to 10 → `value: 10`
2. **Streets from type 9 events** — turn: 1=flop, 2=turn, 3=river
3. **Second board**: run=2 on type 9 events (run-it-twice or double board)
4. **WIN `position`** = pot number (1=main, 2+=side). `runNumber` = string "1"/"2"
5. **Bomb pots**: all players post equally, skip preflop stats. `bombPot: true` on hand and on check/call events
6. **Dead blinds (type 6) are NOT voluntary** — exclude from VPIP
7. **Seat numbers are not sequential** — can be any 1-10 with gaps
8. **`hand` field only present for exporter** — other cards via type 12 SHOW events
9. **Multiple WIN events per hand** — side pots, run-it-twice, double board

### Net profit calculation

Per-street max tracking (bets are cumulative), with antes summed separately:
```
invested = sum(antes) + sum(max_bet_per_street for each street)
net = won + uncalled_return - invested
```

### Stat definitions

- **VPIP**: voluntary preflop money in (call or raise, excluding forced bets)
- **PFR**: preflop raise (type 8 during preflop)
- **3-Bet**: second preflop raise
- **C-Bet**: first flop bet by preflop aggressor
- **AF**: postflop bets / postflop calls
- **WTSD**: went to showdown / saw flop
- **W$SD**: won at showdown / went to showdown
- **WWSF**: won when saw flop (won pot / saw flop, regardless of showdown)
