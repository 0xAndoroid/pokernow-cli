# PokerNow Hand History Parsing

Complete reference for parsing PokerNow JSON hand history exports.

## Top-Level JSON Structure

```json
{
  "generatedAt": "2026-02-24T17:23:11.830Z",   // ISO 8601 export timestamp
  "playerId": "wBcyK_YnY6",                     // ID of the player who exported
  "gameId": "pglF1LvFjtObLvgZ5ctjDDMX-",        // Unique game/table ID
  "hands": [...]                                  // Array of hand objects
}
```

## Hand Object Schema

```json
{
  "id": "x75f0vosvwtl",         // Unique hand ID string
  "handVersion": 2,              // Always 2 in current format
  "number": "1",                 // Hand number (string, sequential)
  "gameType": "th",              // "th" = Texas Hold'em, "omaha" = Omaha
  "cents": false,                // true if stakes are in cents (real money mode)
  "smallBlind": 1,               // Small blind size
  "bigBlind": 1,                 // Big blind size
  "ante": null,                  // Table-wide ante (null if none)
  "straddleSeat": null,          // Seat with mandatory straddle (null if none)
  "dealerSeat": 3,               // Seat number of the dealer/button
  "startedAt": 1771890309670,    // Unix timestamp (milliseconds)
  "bombPot": false,              // true if this is a bomb pot hand
  "sevenDeuceBounty": null,      // Bounty amount for winning with 7-2 (null if disabled)
  "doubleBoard": null,           // Non-null if double board is active
  "players": [...],              // Array of player objects in this hand
  "events": [...]                // Array of event objects in chronological order
}
```

### Player Object

```json
{
  "id": "wBcyK_YnY6",      // Unique player ID (persistent across sessions)
  "seat": 1,                 // Seat number (1-10)
  "name": "Andrew",          // Display name
  "stack": 100,              // Starting stack for this hand (in big blinds)
  "hand": ["7s", "9c"]       // Hole cards (ONLY present for the exporting player + shown hands)
}
```

- `hand` field is **only present** for the player who exported the data. Other players' hands only appear if shown at showdown (via type 12 SHOW events).
- For Hold'em: 2 cards. For Omaha: 4 cards.
- Card format: `"Rank" + "Suit"` where Rank = {2-9, T, J, Q, K, A} and Suit = {s, h, d, c}.
  - Note: In `handDescription` and `handsLabels.d`, "10" is used instead of "T".

## Event Types (Complete Reference)

Events are the core of hand history parsing. Each event has:

```json
{
  "at": 1771890309670,    // Unix timestamp in milliseconds
  "payload": { ... }       // Event-specific payload
}
```

### Type 0 - FOLD

```json
{"type": 0, "seat": 10}
```

Player folds. Always has exactly `type` and `seat`.

### Type 1 - CHECK

```json
{"type": 1, "seat": 10}
{"type": 1, "seat": 10, "value": 3, "bombPot": true}   // in bomb pots
```

Player checks. In bomb pot hands, may include `value` and `bombPot: true`.

### Type 2 - BIG BLIND

```json
{"type": 2, "seat": 10, "value": 1}
```

Forced big blind posting. `value` = amount posted.

### Type 3 - SMALL BLIND

```json
{"type": 3, "seat": 4, "value": 1}
```

Forced small blind posting. `value` = amount posted.

### Type 4 - ANTE

```json
{"type": 4, "seat": 7, "value": 1}
```

Individual ante posting. Appears per-player when antes are active.

### Type 5 - STRADDLE / VOLUNTARY POST

```json
{"type": 5, "seat": 10, "value": 1}
```

Voluntary blind post or straddle. Distinct from `straddleSeat` hand-level field (which tracks mandatory straddle configuration).

### Type 6 - DEAD BLIND

```json
{"type": 6, "seat": 1, "value": 2}
```

Dead blind posted by a player returning to the table or posting out of position.

### Type 7 - CALL

```json
{"type": 7, "seat": 4, "value": 3}
{"type": 7, "seat": 3, "value": 69, "allIn": true}
{"type": 7, "seat": 10, "value": 2, "bombPot": true}
```

Player calls. `value` = total call amount. Optional `allIn: true` if calling puts player all-in. Optional `bombPot: true` in bomb pot hands.

### Type 8 - BET / RAISE

```json
{"type": 8, "seat": 3, "value": 3}
{"type": 8, "seat": 10, "value": 146, "allIn": true}
```

Player bets or raises. `value` = total bet/raise amount (NOT the raise increment -- it's the total amount put in). Optional `allIn: true`.

**Important**: PokerNow does not distinguish between an opening bet and a raise. Both are type 8. You must track bet count per street to determine if it's an open, raise, 3-bet, etc.

### Type 9 - BOARD (Community Cards)

```json
{
  "type": 9,
  "turn": 1,              // 1=flop, 2=turn, 3=river
  "run": 1,               // 1=first/only board, 2=second board (run-it-twice or double board)
  "cards": ["4d", "5d", "6c"],   // Cards dealt this street
  "handsLabels": {
    "1": [{"c": 10}],                              // seat 1: high card
    "4": [{"c": 9, "d": ["K"]}],                   // seat 4: pair of kings
    "10": [{"c": 8, "d": ["A", "2"]}]              // seat 10: two pair, aces and twos
  }
}
```

- `turn`: 1 = flop (3 cards), 2 = turn (1 card), 3 = river (1 card)
- `run`: Board number. Usually 1. Is 2 for run-it-twice second runs or double-board second boards.
- `cards`: Array of new cards dealt on this street.
- `handsLabels`: Current best hand evaluation for each active (non-folded) player. Keys are seat numbers (as strings). Values are arrays:
  - **Single board**: Array with 1 element
  - **Double board**: Array with 2 elements (one per board)

#### `handsLabels` Hand Categories (`c` field)

| c | Hand | Notes |
|---|------|-------|
| 1 | Royal Flush | (theoretical, not observed in sample) |
| 2 | Straight Flush | |
| 3 | Four of a Kind | |
| 4 | Full House | |
| 5 | Flush | |
| 6 | Straight | |
| 7 | Three of a Kind | |
| 8 | Two Pair | `d` = array of 2 rank strings, e.g. `["A", "2"]` |
| 9 | Pair | `d` = array of 1 rank string, e.g. `["K"]` |
| 10 | High Card | |

Lower `c` = stronger hand. The `d` (description) field only appears for Two Pair (c=8) and Pair (c=9). Rank strings use "10" not "T".

### Type 10 - WIN

Two variants depending on whether there was a showdown:

**Showdown win (contested pot):**
```json
{
  "type": 10,
  "pot": 188,
  "seat": 4,
  "value": 188,
  "cards": ["8s", "As", "Kd", "Th"],
  "combination": ["As", "Ad", "8s", "8h", "8d"],
  "handDescription": "Full House, 8's over A's",
  "position": 1,
  "runNumber": "1",
  "hiLo": "h"
}
```

**No-showdown win (uncontested pot):**
```json
{
  "type": 10,
  "seat": 1,
  "value": 31,
  "pot": 31,
  "position": 1
}
```

Fields:
- `pot`: Total pot size for this pot (main or side)
- `seat`: Winning player's seat
- `value`: Amount won (usually equals `pot`)
- `cards`: Winner's hole cards (only at showdown)
- `combination`: The 5 cards making the best hand (only at showdown)
- `handDescription`: Human-readable hand name (only at showdown)
- `position`: Pot number. 1 = main pot, 2 = side pot (integer)
- `runNumber`: Which board/run this win is for. "1" or "2" (string, only at showdown)
- `hiLo`: "h" for high. Only at showdown. Would be "l" for low in hi-lo games.

**Gotcha**: `runNumber` is a **string** ("1", "2"), not an integer. `position` is an **integer**.

### Type 11 - PLAYER ACTION MARKER (Turn to Act)

```json
{"type": 11, "seat": 1}
```

Signals that it is now this player's turn to act. Informational only -- useful for timing tells or reconstructing action order, but not needed for hand analysis. The Python script correctly ignores these.

### Type 12 - SHOW CARDS

```json
{"type": 12, "seat": 4, "cards": ["Kc", "7c"]}
{"type": 12, "seat": 10, "cards": ["3h", "8h", "5h", "4d"]}   // Omaha (4 cards)
```

Player reveals cards at showdown. Appears after type 15 (SHOWDOWN) event. Hold'em shows 2 cards, Omaha shows 4.

### Type 14 - RUN-IT-TWICE VOTE

```json
{
  "type": 14,
  "approved": true,
  "autoApproved": false,
  "approvedSeats": [8, 4],
  "deniedSeats": []
}
```

Records run-it-twice vote outcome. `approved: true` means the remaining board will be dealt twice. Both players must agree (or it can be auto-approved by table settings).

### Type 15 - SHOWDOWN

```json
{"type": 15}
```

Marks the showdown point. Always appears exactly once per hand, even in hands that end without showdown (it still fires before the WIN event). No additional fields.

**Gotcha**: Type 15 fires in EVERY hand, not just contested ones. To determine if a real showdown occurred (multiple players saw cards), check if type 12 (SHOW) events follow it.

### Type 16 - UNCALLED BET RETURN

```json
{"type": 16, "value": 116, "seat": 10}
```

Returns uncalled portion of a bet/raise when all opponents fold. `value` = amount returned. Important for accurate pot/profit calculations.

## Event Type Summary Table

| Type | Name | Key Fields | Notes |
|------|------|-----------|-------|
| 0 | Fold | seat | |
| 1 | Check | seat | May have `bombPot`, `value` in bomb pots |
| 2 | Big Blind | seat, value | |
| 3 | Small Blind | seat, value | |
| 4 | Ante | seat, value | Per-player |
| 5 | Straddle/Post | seat, value | Voluntary post |
| 6 | Dead Blind | seat, value | |
| 7 | Call | seat, value | Optional: `allIn`, `bombPot` |
| 8 | Bet/Raise | seat, value | Optional: `allIn` |
| 9 | Board | turn, run, cards, handsLabels | turn: 1-3, run: 1-2 |
| 10 | Win | seat, value, pot, position | Showdown adds: cards, combination, handDescription, runNumber, hiLo |
| 11 | Action Marker | seat | Player's turn to act (informational) |
| 12 | Show Cards | seat, cards | At showdown |
| 13 | (Unknown) | | Not observed -- may not exist |
| 14 | Run-It-Twice Vote | approved, autoApproved, approvedSeats, deniedSeats | |
| 15 | Showdown | (none) | Fires every hand, even uncontested |
| 16 | Uncalled Bet Return | seat, value | |

## Street Tracking

Streets are determined by type 9 (BOARD) events:

- **Preflop**: Before any board event (turn=1)
- **Flop**: After board event with `turn=1` (3 cards dealt)
- **Turn**: After board event with `turn=2` (1 card dealt)
- **River**: After board event with `turn=3` (1 card dealt)

```python
street = 0  # preflop
for event in events:
    if event['payload']['type'] == 9:  # BOARD
        street = event['payload']['turn']  # 1=flop, 2=turn, 3=river
```

## Parsing Bets vs Raises

PokerNow uses type 8 for both opening bets and raises. Track the raise count per street:

```python
street_raise_count = 0

for event in events:
    payload = event['payload']
    if payload['type'] == 9:  # new street
        street_raise_count = 0
    elif payload['type'] == 8:  # bet/raise
        street_raise_count += 1
        if street == 0:  # preflop
            if street_raise_count == 1:
                # Open raise (first voluntary raise preflop)
                pass
            elif street_raise_count == 2:
                # 3-bet
                pass
            elif street_raise_count == 3:
                # 4-bet
                pass
        else:  # postflop
            if street_raise_count == 1:
                # Opening bet
                pass
            else:
                # Raise / re-raise
                pass
```

## Position Calculation

Positions are derived from `dealerSeat` and the ordered `players` array:

```python
def get_positions(hand):
    """Returns {seat: position_name} mapping."""
    players = sorted(hand['players'], key=lambda p: p['seat'])
    seats = [p['seat'] for p in players]
    n = len(seats)
    dealer_idx = seats.index(hand['dealerSeat'])

    # Positions assigned clockwise from dealer
    # In a full ring: BTN, SB, BB, UTG, UTG+1, ..., CO
    position_names = {
        2: ['BTN/SB', 'BB'],
        3: ['BTN', 'SB', 'BB'],
        4: ['BTN', 'SB', 'BB', 'UTG'],
        5: ['BTN', 'SB', 'BB', 'UTG', 'CO'],
        6: ['BTN', 'SB', 'BB', 'UTG', 'MP', 'CO'],
        7: ['BTN', 'SB', 'BB', 'UTG', 'UTG+1', 'MP', 'CO'],
        8: ['BTN', 'SB', 'BB', 'UTG', 'UTG+1', 'MP', 'MP+1', 'CO'],
        9: ['BTN', 'SB', 'BB', 'UTG', 'UTG+1', 'UTG+2', 'MP', 'MP+1', 'CO'],
        10: ['BTN', 'SB', 'BB', 'UTG', 'UTG+1', 'UTG+2', 'MP', 'MP+1', 'HJ', 'CO'],
    }

    names = position_names.get(n, [f'Seat{i}' for i in range(n)])
    result = {}
    for i in range(n):
        seat = seats[(dealer_idx + i) % n]
        result[seat] = names[i]
    return result
```

## Core Stat Calculations

### VPIP (Voluntarily Put Money In Pot)

Percentage of hands where a player voluntarily put money in preflop (calls or raises, excluding blinds/antes).

```python
# Track during preflop: if player makes type 7 (CALL) or type 8 (BET/RAISE), mark as VPIP
# Do NOT count forced bets (types 2, 3, 4, 5, 6)
vpip = vpip_hands / total_hands * 100
```

### PFR (Pre-Flop Raise)

Percentage of hands where a player raised preflop.

```python
# Track: if player makes type 8 (BET/RAISE) during street 0 (preflop)
pfr = pfr_hands / total_hands * 100
```

### 3-Bet Percentage

```python
# Opportunity: player is active when the 2nd raise (preflop_raise_count == 1 already happened)
# Action: player makes the 2nd preflop raise (preflop_raise_count becomes 2)
three_bet_pct = three_bets / three_bet_opportunities * 100
```

### Aggression Factor (AF)

```python
af = (total_bets + total_raises) / total_calls
# Or simplified (since type 8 covers both bets and raises):
af = total_type_8_actions / total_type_7_actions
```

### WTSD (Went To Showdown)

Percentage of hands where player saw the flop and then went to showdown.

```python
# Denominator: hands where player saw flop (was active when turn=1 board dealt)
# Numerator: hands where player was active at showdown (not folded when type 15 fires)
# Only count if type 12 (SHOW) events follow -- otherwise it's an uncontested "showdown"
wtsd = went_to_showdown / saw_flop * 100
```

### W$SD (Won Money at Showdown)

```python
wsd = won_at_showdown / went_to_showdown * 100
```

### C-Bet (Continuation Bet)

```python
# Opportunity: player was the preflop aggressor AND sees the flop AND has not folded
# Action: player makes the first type 8 (BET) on the flop (street=1)
cbet_pct = cbets / cbet_opportunities * 100
```

### Win Rate

```python
# Total won in BB per hand
# Note: "total_won" from WIN events counts gross winnings, not net profit
# For net profit: need to subtract what the player put into the pot
bb_per_hand = total_won / hands_played

# For more accurate net calculation, track investments per hand:
# invested = sum of all type 2,3,4,5,6,7,8 values for that player in the hand
# net = win_value - invested  (or just -invested if they didn't win)
```

## Handling Special Situations

### Side Pots

Multiple WIN events with different `position` values in the same hand:

```python
for event in events:
    if event['payload']['type'] == 10:  # WIN
        position = event['payload']['position']
        if position == 1:
            # Main pot winner
            pass
        elif position == 2:
            # Side pot winner
            pass
```

Side pots occur when a player goes all-in for less than the full bet. The main pot (position=1) includes all players; the side pot (position=2) excludes the short-stacked all-in player.

### Run-It-Twice

When approved (type 14 with `approved: true`), the remaining streets are dealt twice:

```python
# Board events come in pairs: run=1 then run=2 for each remaining street
# WIN events have runNumber "1" or "2"
# Each run awards half the pot independently

for event in events:
    payload = event['payload']
    if payload['type'] == 9:  # BOARD
        run = payload['run']        # 1 or 2
        turn = payload['turn']      # 1=flop, 2=turn, 3=river
    elif payload['type'] == 10:  # WIN
        run_number = payload.get('runNumber')  # "1" or "2" (string!)
```

### Bomb Pots

All players post a predetermined amount, and the flop is dealt immediately (no preflop betting):

```python
if hand['bombPot']:
    # Skip preflop analysis for this hand
    # Events may have bombPot: true on check/call actions
    # Often paired with doubleBoard and Omaha gameType
    pass
```

### Omaha Hands

```python
if hand['gameType'] == 'omaha':
    # Players have 4 hole cards instead of 2
    # hand['players'][i]['hand'] has 4 elements
    # Type 12 SHOW events also have 4-card arrays
    pass
```

### Double Boards

In double board hands, board events come in pairs (run=1 and run=2) for each street. `handsLabels` arrays have 2 elements per seat:

```python
# handsLabels for double board:
# "1": [{"c": 9, "d": ["K"]}, {"c": 6}]
#        ^--- board 1 hand      ^--- board 2 hand
```

### Straddle Handling

Two distinct concepts:
1. **`straddleSeat`** (hand-level): Mandatory straddle seat from table configuration
2. **Type 5 events**: Voluntary straddle/post by individual players

When straddles are present, the effective "big blind" for preflop action is the straddle amount, and preflop action order shifts accordingly.

### Dead Blinds (Type 6)

Players who sit out and return must post a dead blind. This is NOT voluntary -- do NOT count it toward VPIP. The dead blind goes into the pot but the player hasn't voluntarily entered.

## Common Parsing Patterns

### Iterating All Hands

```python
import json
from collections import defaultdict

with open('hands.json') as f:
    data = json.load(f)

for hand in data['hands']:
    hand_num = hand['number']
    players = {p['seat']: p for p in hand['players']}
    events = hand['events']

    for event in events:
        payload = event['payload']
        event_type = payload['type']
        seat = payload.get('seat')
        # ... process
```

### Reconstructing Board State

```python
def get_board(events, run=1):
    """Returns the full community board for a given run."""
    board = []
    for event in events:
        payload = event['payload']
        if payload['type'] == 9 and payload['run'] == run:
            board.extend(payload['cards'])
    return board

# Example: ["4d", "5d", "6c", "2h", "Ad"]
```

### Calculating Net Profit Per Hand

```python
def net_profit(events, seat):
    """Calculate net profit/loss for a specific seat in a hand."""
    invested = 0
    won = 0
    returned = 0

    for event in events:
        payload = event['payload']
        if payload.get('seat') != seat:
            continue

        t = payload['type']
        if t in (2, 3, 4, 5, 6, 7, 8):  # all money-in events
            invested += payload.get('value', 0)
        elif t == 10:  # win
            won += payload.get('value', 0)
        elif t == 16:  # uncalled bet return
            returned += payload.get('value', 0)

    return won + returned - invested
```

### Detecting Real Showdowns

```python
def had_real_showdown(events):
    """Check if hand had a contested showdown (not just uncontested end)."""
    showdown_seen = False
    show_count = 0
    for event in events:
        t = event['payload']['type']
        if t == 15:
            showdown_seen = True
        elif t == 12 and showdown_seen:
            show_count += 1
    return show_count >= 2  # At least 2 players showed cards
```

### Finding the Preflop Aggressor

```python
def preflop_aggressor(events, seat_to_name):
    """Returns the seat of the last preflop raiser."""
    aggressor = None
    for event in events:
        payload = event['payload']
        if payload['type'] == 9:  # board dealt = preflop over
            break
        if payload['type'] == 8:  # bet/raise
            aggressor = payload['seat']
    return aggressor
```

## Edge Cases and Gotchas

1. **Type 15 (SHOWDOWN) fires every hand** -- even uncontested ones. Don't use it alone to determine if a showdown happened. Check for type 12 (SHOW) events after it.

2. **`value` in type 8 is total, not increment** -- A raise from 3 to 10 shows `value: 10`, not `value: 7`.

3. **`runNumber` is a string** ("1", "2"), while `position` is an integer. Don't mix these up.

4. **Bomb pots skip preflop** -- Don't count bomb pot hands in VPIP/PFR calculations. All players post equally and go directly to flop.

5. **Dead blinds (type 6) are not voluntary** -- Don't count them toward VPIP.

6. **Type 11 (action markers) are noise** -- Safely skip them in all analysis.

7. **Type 14 (run-it-twice votes) are informational** -- Useful for knowing if run-it-twice was offered/accepted, but not needed for standard stats.

8. **Players array only has hole cards for the exporter** -- Other players' cards only appear in type 12 (SHOW) events at showdown.

9. **Seat numbers are NOT sequential** -- Seats can be any number 1-10 with gaps. Always use the actual seat values from the players array.

10. **Multiple WIN events per hand** are normal -- can happen from side pots, run-it-twice, or double boards.

11. **`straddleSeat` vs type 5 events** -- These track different things. `straddleSeat` is the mandatory straddle seat configuration; type 5 events are voluntary posts. Many hands with `straddleSeat` set don't have type 5 events.

12. **Omaha hands in this dataset are always bomb pots** -- Don't assume Omaha is always a bomb pot in general, but be aware that mixed-game sessions can switch `gameType` mid-file.

13. **The `hand` field on players may have 2 or 4 cards** -- 2 for Hold'em, 4 for Omaha. Same for type 12 SHOW cards arrays.

14. **Card notation inconsistency** -- Cards use "T" for ten (e.g., "Ts"), but `handDescription` and `handsLabels.d` use "10".

15. **`combination` in WIN events is the 5-card best hand** -- NOT the hole cards. It's the evaluated best 5-card combination from hole cards + board.

16. **Spurious fold events (type 0) are pervasive** -- Approximately 55% of hands (275/501 in the Feb 24 dataset) contain type 0 FOLD events that are noise. A player may "fold" but then act later in the same hand, or fold events appear for players who already folded. There are 621 total spurious fold instances across the dataset. They appear in three patterns:
    - After board events (type 9): 314 instances -- the most common pattern. Players who have already folded generate another fold event when a new street is dealt.
    - Before any board event: 82 instances -- folds that fire at the start of the hand for players not in the hand, or duplicate folds during preflop.
    - Mixed/other positions: 213 instances -- scattered throughout hand events.

    **Impact on analysis**: If you naively count type 0 events to track folds, WTSD, or fold-to-cbet, you will get wildly inflated fold counts. To handle this correctly, maintain a set of "active" (non-folded) players and only register the FIRST fold per player per hand. Alternatively, track which players are still live by checking who has pending action markers (type 11) or who appears in later betting events.

17. **Partial card shows (type 12 with None values)** -- In approximately 70 instances, type 12 SHOW events contain `None` instead of a card string in the `cards` array (e.g., `["Kc", None]`). This happens when a player shows only one of their two hole cards. Your parsing must handle `None` values in the cards array -- don't assume all elements are valid card strings.

18. **Net profit calculation requires max-per-street tracking** -- Because type 8 `value` is the cumulative total bet on that street (not the increment), you cannot simply sum all type 2/3/4/5/6/7/8 values for a player to get their investment. A player who posts BB (type 2, value 1) and then raises (type 8, value 10) has invested 10, not 11. The correct approach: for each player on each street, take the **maximum** `value` across all their monetary events (types 2-8) on that street. Sum those per-street maximums to get total investment. Then: `net_profit = win_value + uncalled_return - total_investment`.

    ```python
    def net_profit(events, seat):
        street = 0
        street_investments = defaultdict(lambda: defaultdict(float))
        won = 0
        returned = 0
        for event in events:
            payload = event['payload']
            if payload['type'] == 9:
                street = payload['turn']
            if payload.get('seat') != seat:
                continue
            t = payload['type']
            if t in (2, 3, 4, 5, 6, 7, 8):
                val = payload.get('value', 0)
                street_investments[street][seat] = max(street_investments[street][seat], val)
            elif t == 10:
                won += payload.get('value', 0)
            elif t == 16:
                returned += payload.get('value', 0)
        invested = sum(v for s in street_investments.values() for v in s.values())
        return won + returned - invested
    ```

19. **Run-it-twice event ordering** -- The type 14 (RIT vote) event appears AFTER the all-in and any SHOW events but BEFORE the remaining board events. The sequence is: all-in action → type 12 SHOW (players reveal cards) → type 14 RIT vote → type 9 BOARD (run 1) → type 9 BOARD (run 2) → ... → type 10 WIN (runNumber "1") → type 10 WIN (runNumber "2"). Each run awards its portion of the pot independently.

20. **Dead blinds can appear without corresponding BB/SB events** -- Type 6 (dead blind) events sometimes fire for a player in a hand where no type 2 (BB) or type 3 (SB) event exists for anyone. This happens when a player returns to the table and posts a dead blind in lieu of waiting for the blind to reach them. Don't assume every hand has exactly one BB and one SB event.

21. **Bomb pot betting uses bombPot flag** -- In bomb pot hands, type 1 (CHECK) and type 7 (CALL) events may include `bombPot: true` in the payload. The `value` on these events represents the bomb pot ante each player posts. These are forced posts, not voluntary actions -- do not count them toward VPIP, PFR, or any voluntary stat.

22. **Type 1 (CHECK) events may be absent when a street checks through** -- When all active players check on a street (e.g., the flop checks through to the turn), PokerNow may not emit type 1 CHECK events at all. Instead, the only events between consecutive type 9 BOARD events are spurious type 0 FOLD events. This means you cannot rely on the presence of type 1 events to detect checks. To determine if a street was checked through, check for the *absence* of type 7 (CALL) and type 8 (BET/RAISE) events between two consecutive BOARD events. If no monetary action occurred on a street, it was checked through regardless of whether type 1 events exist. This also means that calculating a player's check frequency by counting type 1 events will undercount -- a player who checks 74% of flops may show zero type 1 events if most of those checks result in the street checking through entirely.

## Player Classification Thresholds

Based on the existing `analyze_hands.py` script's `classify_player()` function:

| Type | VPIP | PFR | Other |
|------|------|-----|-------|
| Maniac | >55% | >35% | |
| LAG (Loose-Aggressive) | >45% | >28% | |
| Fish / Calling Station | >40% | <18% | |
| TAG (Shark) | <25% | >18% | |
| Rock / Nit | <20% | <12% | |
| Whale / Loose-Passive | >35% | | VPIP-PFR gap >20 |
| ATM / Massive Fish | >60% | | |

Additional trait indicators:
- AF > 3.5 = hyper-aggressive, AF < 0.8 = very passive
- VPIP-PFR gap > 25 = passive preflop
- WTSD > 40% = showdown monkey
- W$SD > 55% = strong showdown game
- 3-bet > 15% = very wide 3-betting range

## Existing Script Reference

The file `analyze_hands.py` (in this directory) implements:

- **`analyze_hands(filepath)`**: Full stat extraction -- VPIP, PFR, 3-bet, 4-bet, C-bet, AF, WTSD, W$SD, open raise, limp, cold call, all-in preflop tracking
- **`classify_player(stats)`**: Categorizes players into archetypes (Maniac, LAG, Fish, TAG, Rock, Whale, ATM, Average) with trait annotations
- **`calculate_skill_score(stats)`**: Composite 0-100 skill score based on how close stats are to optimal ranges
- **Main output**: Per-player stat tables, player rankings by type, exploit recommendations, and overall skill rankings

### What the existing script does NOT handle:
- Position-aware stats (no UTG vs BTN VPIP breakdown)
- Per-street aggression tracking (only preflop/postflop split)
- Run-it-twice detection or handling
- Bomb pot filtering (bomb pots are counted in stats, skewing VPIP)
- Side pot tracking
- Omaha hand detection
- Net profit calculation (uses gross `total_won` from WIN events, not net)
- Session/time-based analysis
- Hand-by-hand replay
- Type 16 (uncalled bet returns) for accurate profit calculation
- Fold-to-3-bet tracking
- Squeeze play detection
- Check-raise frequency
- Donk bet detection (non-aggressor leading out)
