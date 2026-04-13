# Scripts

`outcome-market-demo.ts` is an end-to-end Anchor client example for `OutcomeMarket`.

It demonstrates:

- permissionless market initialization
- arbitrary collateral decimals via a fresh demo SPL mint
- `split`
- `merge` before resolution
- permissionless `set_start_price` by a different signer
- permissionless `resolve` by a different signer
- `claim` with only the winning token account
- `merge` after resolution
- all current market types (`UpDown`, `AbovePrice`, `BelowPrice`, `WithinRange`)

Run it with:

```bash
yarn demo:ts
```

Important:

- the target cluster must have your `OutcomeMarket` program deployed
- the target cluster must also have the Pyth Solana Receiver available
- for local validators, set `PYTH_RECEIVER_PROGRAM_ID`, `PYTH_WORMHOLE_PROGRAM_ID`, and `PYTH_PUSH_ORACLE_PROGRAM_ID`
