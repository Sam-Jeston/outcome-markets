# OutcomeMarket

`OutcomeMarket` is an Anchor program for binary outcome markets settled against Pyth price updates.

This README documents the behavior of the program as it is currently implemented in this repository. It intentionally describes actual on-chain behavior, including important edge cases and places where the current implementation is looser than an idealized market design.

## Overview

Each market:

- is permissionlessly initialized by any signer
- is keyed by a Pyth feed id, a `start_time`, an `end_time`, and a market type
- holds collateral in a vault
- mints market-specific `YES` and `NO` SPL tokens
- resolves to `YES` or `NO` using a Pyth `PriceUpdateV2` account

Economically, the program implements conditional tokens:

- `split(amount)` deposits `amount` collateral and mints `amount` YES plus `amount` NO
- `merge(amount)` burns `amount` YES and `amount` NO and returns `amount` collateral
- `claim(amount)` burns `amount` winning tokens and returns `amount` collateral after resolution

All amounts are raw token base units. With 6-decimal collateral, `1 USDC` is `1_000_000`. With 9-decimal collateral, one whole token is `1_000_000_000`.

## Current Important Behaviors

These are the most important implementation details to understand:

1. The program does not enforce a canonical USDC mint address. It accepts any SPL mint and uses that mint's native decimals.
2. The program does not fetch Pyth data or pay Pyth update fees itself. Callers must supply an already-created `PriceUpdateV2` account.
3. `set_start_price` and `resolve` require both the on-chain `Clock` and the supplied Pyth update `publish_time` to have reached the relevant market timestamp.
4. The program does not try to find the first update after `start_time` or `end_time`, and it does not require the closest update to those boundaries.
5. The caller still chooses which qualifying Pyth update account is used, as long as it is fully verified and its `publish_time` is at or after the relevant boundary.
6. `merge` is allowed both before and after resolution. A matched YES/NO pair remains redeemable for collateral at all times.

## Market Identity And Accounts

The market PDA is derived from:

- `"market"`
- `price_feed_id`
- `end_time`
- encoded market type bytes
- `start_time`

The market-specific token PDAs are:

- YES mint: `[market, "yes"]`
- NO mint: `[market, "no"]`
- collateral vault: `[market, "vault"]`

The market PDA is also the mint authority for YES and NO, and the token authority for the collateral vault.

The `OutcomeMarket` account stores:

- `creator`
- `price_feed_id`
- `market_type_seed`
- `market_type`
- `collateral_mint`
- `yes_mint`
- `no_mint`
- `collateral_vault`
- `start_time`
- `end_time`
- optional `start_price`
- optional `resolved_price`
- `resolution`
- PDA bumps

`creator` is informational only. It does not grant any special permissions after initialization.

## Collateral Model

The program currently uses a "USDC-like" collateral model:

- any SPL collateral mint is accepted
- the YES and NO mints inherit the collateral mint decimals
- the program does not verify that the mint is the canonical Solana USDC mint

In tests, the fixture mint is the mainnet USDC mint:

- `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`

## Market Types

### `AbovePrice { price, exponent }`

Resolves `YES` when:

- resolved price exponent equals `exponent`
- resolved price is strictly greater than `price`

Otherwise resolves `NO`.

Equal-to-threshold resolves `NO`.

### `BelowPrice { price, exponent }`

Resolves `YES` when:

- resolved price exponent equals `exponent`
- resolved price is strictly less than `price`

Otherwise resolves `NO`.

Equal-to-threshold resolves `NO`.

### `WithinRange { lower_price, upper_price, exponent }`

Resolves `YES` when:

- resolved price exponent equals `exponent`
- resolved price is greater than or equal to `lower_price`
- resolved price is less than or equal to `upper_price`

Otherwise resolves `NO`.

This range is inclusive on both ends.

### `UpDown`

Resolves `YES` when:

- a `start_price` has been set
- resolved price exponent equals the stored start price exponent
- resolved price is strictly greater than the stored start price

Otherwise resolves `NO`.

Equal start and end prices resolve `NO`.

Unlike the threshold market types, `UpDown` does not pre-store an exponent in the market definition. It only requires start and end price exponents to match each other.

## Oracle Behavior

The program reads prices from `pyth_solana_receiver_sdk::price_update::PriceUpdateV2`.

For a price update to be accepted:

- the account must deserialize as `PriceUpdateV2`
- the update must contain the market's `price_feed_id`
- the update verification level must be at least `VerificationLevel::Full`

The program then uses `get_price_unchecked(feed_id)` on that update account.

What the program does not currently do:

- it does not invoke the Pyth receiver program
- it does not pay the Pyth update fee on-chain
- it does not compare multiple candidate updates
- it does not enforce "closest update to boundary"
- it does not check confidence intervals
- it does not check a max age beyond the timestamp threshold

This means the caller chooses which qualifying Pyth update account is used for `set_start_price` and `resolve`.

## Instruction Behavior

### `initialize_market`

Behavior:

- permissionless
- payer funds creation of the market account, YES mint, NO mint, and collateral vault
- stores the chosen market parameters in the market account

Validation:

- `end_time` must be greater than `start_time`
- for `WithinRange`, `lower_price <= upper_price`
- current `Clock::unix_timestamp` must be strictly less than `end_time`

Important notes:

- `start_time` may be in the future
- `start_time` may also already be in the past, as long as `end_time` is still in the future
- no other permissioning exists
- no canonical-USDC-address check exists
- the YES and NO mints inherit whatever decimals the collateral mint uses

### `split(amount)`

Behavior:

- transfers `amount` collateral from the user's collateral token account into the market vault
- mints `amount` YES to the user's YES token account
- mints `amount` NO to the user's NO token account

Validation:

- `amount > 0`
- market must not already be resolved
- current `Clock::unix_timestamp` must be strictly less than `end_time`
- provided mint and vault accounts must match the market
- the user collateral account must be owned by the signer and match the collateral mint
- the YES and NO token accounts must be owned by the signer and match the YES/NO mints

Important notes:

- `split` is not hard-coded to exactly 1 USDC; it accepts any positive amount
- `split` does not care about `start_time`
- the program does not create token accounts for the user; the user must already have collateral, YES, and NO token accounts

### `merge(amount)`

Behavior:

- burns `amount` YES from the user's YES token account
- burns `amount` NO from the user's NO token account
- transfers `amount` collateral from the market vault back to the user's collateral token account

Validation:

- `amount > 0`
- provided mint and vault accounts must match the market
- the user collateral account must be owned by the signer and match the collateral mint
- the YES and NO token accounts must be owned by the signer and match the YES/NO mints

Important notes:

- `merge` is allowed before resolution
- `merge` is also allowed after resolution
- `merge` is allowed after `end_time`
- this means a matched YES/NO pair is always redeemable for collateral

### `set_start_price`

Behavior:

- stores a `RecordedPrice` in `market.start_price`

Validation:

- market type must be `UpDown`
- market must not already be resolved
- start price must not already be set
- current `Clock::unix_timestamp` must be greater than or equal to `start_time`
- supplied Pyth update must be fully verified
- supplied Pyth update must match the market feed id
- supplied Pyth update `publish_time` must be greater than or equal to `start_time`

Important notes:

- any signer can call it
- it can only be set once
- the program requires both clock time and oracle publish time to have reached `start_time`
- the program does not require the earliest qualifying update
- the chosen start price is "first submitted valid update wins", not "first oracle update after start time"
- because only `publish_time >= start_time` is enforced, the stored start price may come from substantially later than `start_time`
- it may even be set after `end_time`, as long as the market has not yet been resolved

### `resolve`

Behavior:

- stores `market.resolution`
- stores `market.resolved_price`

Validation:

- market must not already be resolved
- current `Clock::unix_timestamp` must be greater than or equal to `end_time`
- supplied Pyth update must be fully verified
- supplied Pyth update must match the market feed id
- supplied Pyth update `publish_time` must be greater than or equal to `end_time`
- for `UpDown`, `start_price` must already exist
- exponent matching rules described in the market type section must pass

Important notes:

- any signer can call it
- it can only be called once successfully
- the program requires both clock time and oracle publish time to have reached `end_time`
- the program does not require the earliest qualifying update after `end_time`
- the caller chooses the qualifying update account
- for `UpDown`, if no one has set the start price yet, resolution fails until `set_start_price` succeeds

### `claim(amount)`

Behavior:

- if the market resolved `YES`, burns `amount` YES from the provided outcome token account
- if the market resolved `NO`, burns `amount` NO from the provided outcome token account
- transfers `amount` collateral from the vault to the user's collateral account

Validation:

- `amount > 0`
- market must already be resolved
- provided collateral mint and vault accounts must match the market
- the provided outcome mint must match the resolved market outcome
- the user collateral account must be owned by the signer and match the collateral mint
- the provided outcome token account must be owned by the signer and match the provided outcome mint

Important notes:

- `claim` works in arbitrary positive amounts, not only 1 token at a time
- only the winning side is burned
- the losing side is untouched and remains in the account as a worthless balance
- `claim` no longer requires the losing-side token account

## Resolution And Redemption Semantics

The settlement model is:

- one unit of collateral can be split into one YES plus one NO
- one YES plus one NO can always be merged back into one unit of collateral
- after resolution, one unit of the winning token can be claimed for one unit of collateral

After resolution:

- a user holding only winners can `claim`
- a user holding matched YES/NO pairs can still `merge`
- a user holding only losers cannot redeem them

No fees are charged by the program for splitting, merging, resolving, or claiming.

## Account Requirements For Clients

Clients must provide the correct token accounts themselves. The program does not create ATAs or token accounts for users.

In practice:

- before `split`, the user needs a collateral token account plus YES and NO token accounts
- before `merge`, the user needs those same three accounts
- before `claim`, the user needs a collateral token account plus only the winning-side token account

## Current Limitations And Non-Features

The current implementation does not include:

- strict enforcement of the canonical USDC mint address
- on-chain Pyth fee payment flow
- automatic creation of user token accounts
- admin controls
- pausing
- disputes
- market cancellation
- market closure / account cleanup instructions
- event emission
- oracle selection logic such as "closest to boundary" or "first update after boundary"

## Build And Test

Build the program:

```bash
anchor build
```

Run the LiteSVM integration tests:

```bash
cargo test -p outcome-markets --test integration -- --nocapture
```

Or via the package script:

```bash
yarn test:litesvm
```

The integration tests load the compiled program binary from:

```text
target/deploy/outcome_markets.so
```

If you want to point tests at a different compiled artifact, set:

```text
OUTCOME_MARKETS_SO=/path/to/outcome_markets.so
```

Current integration tests cover:

- initialize -> split -> merge round trip
- split -> merge round trip with a non-6-decimal collateral mint
- `UpDown` start price setting, resolution, and claim
- `set_start_price` clock gating
- `resolve` clock gating
- inclusive `WithinRange` resolution at the upper boundary
