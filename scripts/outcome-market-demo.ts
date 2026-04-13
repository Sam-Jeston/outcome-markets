/// <reference types="node" />

import * as anchor from "@coral-xyz/anchor";
import {
  createMint,
  getAccount,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

import type { AnchorProvider as AnchorProviderType } from "@coral-xyz/anchor";
import type BN from "bn.js";
import type {
  Connection as Web3Connection,
  Keypair as Web3Keypair,
  PublicKey as Web3PublicKey,
  Signer as Web3Signer,
  TransactionInstruction,
  VersionedTransaction,
} from "@solana/web3.js";

import type { OutcomeMarkets } from "../target/types/outcome_markets";

const {
  BN: AnchorBN,
  Program,
  AnchorProvider,
  Wallet,
  web3: {
    Connection,
    Keypair,
    LAMPORTS_PER_SOL,
    PublicKey,
    SystemProgram,
    SYSVAR_RENT_PUBKEY,
    Transaction,
  },
} = anchor;

const DEFAULT_PRICE_FEED_ID =
  "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43"; // BTC/USD
const DEFAULT_HERMES_URL = "https://hermes.pyth.network";
const DEFAULT_START_DELAY_SECONDS = 12;
const DEFAULT_END_DELAY_SECONDS = 30;
const DEFAULT_COLLATERAL_DECIMALS = 9;
const MARKET_SEED = Buffer.from("market");
const YES_MINT_SEED = Buffer.from("yes");
const NO_MINT_SEED = Buffer.from("no");
const COLLATERAL_VAULT_SEED = Buffer.from("vault");

type MarketSpec =
  | { kind: "upDown" }
  | { kind: "abovePrice"; price: BN; exponent: number }
  | { kind: "belowPrice"; price: BN; exponent: number }
  | {
      kind: "withinRange";
      lowerPrice: BN;
      upperPrice: BN;
      exponent: number;
    };

type DemoMarket = {
  label: string;
  spec: MarketSpec;
  splitAmount: number;
  marketPda: Web3PublicKey;
  yesMint: Web3PublicKey;
  noMint: Web3PublicKey;
  collateralVault: Web3PublicKey;
  yesAta: Web3PublicKey;
  noAta: Web3PublicKey;
};

type HermesPriceUpdate = any;

function installRpcWebsocketsCompatShim() {
  const Module = require("module") as typeof import("module") & {
    _resolveFilename: (
      request: string,
      parent: NodeModule | null | undefined,
      isMain: boolean,
      options?: unknown
    ) => string;
  };
  const compatTargets: Record<string, string> = {
    "rpc-websockets/dist/lib/client": require.resolve(
      "../node_modules/jito-ts/node_modules/rpc-websockets/dist/lib/client.cjs"
    ),
    "rpc-websockets/dist/lib/client/websocket": require.resolve(
      "../node_modules/jito-ts/node_modules/rpc-websockets/dist/lib/client/websocket.cjs"
    ),
  };

  if (
    (Module._resolveFilename as unknown as { __outcomeDemoPatched?: boolean })
      .__outcomeDemoPatched
  ) {
    return;
  }

  const originalResolveFilename = Module._resolveFilename.bind(Module);
  const replacement = (
    request: string,
    parent: NodeModule | null | undefined,
    isMain: boolean,
    options?: unknown
  ) => {
    if (compatTargets[request]) {
      return compatTargets[request];
    }

    return originalResolveFilename(request, parent, isMain, options);
  };

  (
    replacement as typeof replacement & { __outcomeDemoPatched: boolean }
  ).__outcomeDemoPatched = true;
  Module._resolveFilename = replacement;
}

function expandHome(filePath: string): string {
  if (!filePath.startsWith("~/")) {
    return filePath;
  }

  return path.join(os.homedir(), filePath.slice(2));
}

function readKeypair(filePath: string): Web3Keypair {
  const secretKey = JSON.parse(fs.readFileSync(filePath, "utf8")) as number[];
  return Keypair.fromSecretKey(Uint8Array.from(secretKey));
}

function readNumberEnv(name: string, fallback: number): number {
  const raw = process.env[name];
  if (!raw) {
    return fallback;
  }

  const parsed = Number(raw);
  if (!Number.isFinite(parsed)) {
    throw new Error(`Environment variable ${name} must be numeric.`);
  }

  return parsed;
}

function readOptionalPubkeyEnv(name: string): Web3PublicKey | undefined {
  const raw = process.env[name];
  return raw ? new PublicKey(raw) : undefined;
}

function toI64LeBytes(value: BN | number | string): Buffer {
  const bn = AnchorBN.isBN(value) ? value : new AnchorBN(String(value), 10);
  return bn.toTwos(64).toArrayLike(Buffer, "le", 8);
}

function toFeedIdBytes(feedIdHex: string): Buffer {
  const normalized = feedIdHex.startsWith("0x")
    ? feedIdHex.slice(2)
    : feedIdHex;
  const bytes = Buffer.from(normalized, "hex");
  if (bytes.length !== 32) {
    throw new Error(
      `Expected a 32-byte feed id, received ${bytes.length} bytes.`
    );
  }

  return bytes;
}

function marketTypeSeedBytes(spec: MarketSpec): Buffer {
  const bytes = Buffer.alloc(21);

  switch (spec.kind) {
    case "abovePrice":
      bytes[0] = 0;
      bytes.writeInt32LE(spec.exponent, 1);
      toI64LeBytes(spec.price).copy(bytes, 5);
      return bytes;
    case "belowPrice":
      bytes[0] = 1;
      bytes.writeInt32LE(spec.exponent, 1);
      toI64LeBytes(spec.price).copy(bytes, 5);
      return bytes;
    case "withinRange":
      bytes[0] = 2;
      bytes.writeInt32LE(spec.exponent, 1);
      toI64LeBytes(spec.lowerPrice).copy(bytes, 5);
      toI64LeBytes(spec.upperPrice).copy(bytes, 13);
      return bytes;
    case "upDown":
      bytes[0] = 3;
      return bytes;
  }
}

function toAnchorMarketType(spec: MarketSpec): unknown {
  switch (spec.kind) {
    case "abovePrice":
      return {
        abovePrice: {
          price: spec.price,
          exponent: spec.exponent,
        },
      };
    case "belowPrice":
      return {
        belowPrice: {
          price: spec.price,
          exponent: spec.exponent,
        },
      };
    case "withinRange":
      return {
        withinRange: {
          lowerPrice: spec.lowerPrice,
          upperPrice: spec.upperPrice,
          exponent: spec.exponent,
        },
      };
    case "upDown":
      return { upDown: {} };
  }
}

function deriveMarketPdas(
  programId: Web3PublicKey,
  priceFeedId: Buffer,
  startTime: number,
  endTime: number,
  spec: MarketSpec
) {
  const market = PublicKey.findProgramAddressSync(
    [
      MARKET_SEED,
      priceFeedId,
      toI64LeBytes(endTime),
      marketTypeSeedBytes(spec),
      toI64LeBytes(startTime),
    ],
    programId
  )[0];

  const yesMint = PublicKey.findProgramAddressSync(
    [market.toBuffer(), YES_MINT_SEED],
    programId
  )[0];
  const noMint = PublicKey.findProgramAddressSync(
    [market.toBuffer(), NO_MINT_SEED],
    programId
  )[0];
  const collateralVault = PublicKey.findProgramAddressSync(
    [market.toBuffer(), COLLATERAL_VAULT_SEED],
    programId
  )[0];

  return { market, yesMint, noMint, collateralVault };
}

function rawTokens(wholeTokens: number, decimals: number): number {
  return wholeTokens * 10 ** decimals;
}

function formatUiAmount(rawAmount: bigint | number, decimals: number): string {
  const amount =
    typeof rawAmount === "bigint" ? rawAmount.toString() : String(rawAmount);
  const negative = amount.startsWith("-");
  const digits = negative ? amount.slice(1) : amount;
  const padded = digits.padStart(decimals + 1, "0");
  const whole = padded.slice(0, -decimals) || "0";
  const fraction = padded.slice(-decimals).replace(/0+$/, "");

  return `${negative ? "-" : ""}${whole}${fraction ? `.${fraction}` : ""}`;
}

function enumVariant(value: unknown): string {
  if (!value || typeof value !== "object") {
    return String(value);
  }

  const keys = Object.keys(value as Record<string, unknown>);
  return keys[0] ?? "unknown";
}

async function sleep(ms: number) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitUntil(unixTimestamp: number, label: string) {
  for (;;) {
    const now = Math.floor(Date.now() / 1000);
    if (now >= unixTimestamp) {
      return;
    }

    const remaining = unixTimestamp - now;
    console.log(`[wait] ${label} in ${remaining}s`);
    await sleep(Math.min(remaining, 5) * 1_000);
  }
}

async function fetchBoundaryPriceUpdate(
  hermes: any,
  feedIdHex: string,
  boundaryTime: number,
  label: string
): Promise<HermesPriceUpdate> {
  let lastError: unknown;

  for (let attempt = 1; attempt <= 15; attempt += 1) {
    try {
      const update = await hermes.getPriceUpdatesAtTimestamp(
        boundaryTime,
        [feedIdHex],
        { encoding: "base64", parsed: true }
      );

      const parsed = update.parsed?.[0];
      const prevPublishTime = parsed?.metadata?.prev_publish_time;
      const publishTime = parsed?.price.publish_time;

      if (
        prevPublishTime !== undefined &&
        prevPublishTime !== null &&
        publishTime !== undefined &&
        prevPublishTime < boundaryTime &&
        boundaryTime <= publishTime
      ) {
        console.log(
          `[oracle] ${label}: prev_publish_time=${prevPublishTime}, publish_time=${publishTime}`
        );
        return update;
      }

      lastError = new Error(
        `${label} update did not satisfy prev_publish_time < boundary <= publish_time.`
      );
    } catch (error) {
      lastError = error;
    }

    console.log(
      `[oracle] ${label}: waiting for Hermes historical update (attempt ${attempt}/15)`
    );
    await sleep(2_000);
  }

  throw lastError instanceof Error
    ? lastError
    : new Error(`Unable to fetch ${label} price update from Hermes.`);
}

async function fetchLatestParsedPrice(hermes: any, feedIdHex: string) {
  const latest = await hermes.getLatestPriceUpdates([feedIdHex], {
    encoding: "base64",
    parsed: true,
  });
  const parsed = latest.parsed?.[0];

  if (!parsed) {
    throw new Error("Hermes did not return a parsed latest price update.");
  }

  return parsed;
}

async function fetchTokenAmount(
  connection: Web3Connection,
  account: Web3PublicKey
): Promise<bigint> {
  const tokenAccount = await getAccount(connection, account, "confirmed");
  return tokenAccount.amount;
}

function logDivider(title: string) {
  console.log(`\n=== ${title} ===`);
}

async function logCollateralBalance(
  connection: Web3Connection,
  label: string,
  collateralAta: Web3PublicKey,
  decimals: number
) {
  const amount = await fetchTokenAmount(connection, collateralAta);
  console.log(
    `[balance] ${label}: ${formatUiAmount(
      amount,
      decimals
    )} collateral (${amount.toString()} raw)`
  );
}

async function logMarketState(
  program: anchor.Program<OutcomeMarkets>,
  market: DemoMarket
) {
  const account = (await program.account.outcomeMarket.fetch(
    market.marketPda
  )) as any;
  const startPrice = account.startPrice;
  const resolvedPrice = account.resolvedPrice;

  console.log(
    `[market] ${market.label}: resolution=${enumVariant(
      account.resolution
    )}, startPrice=${
      startPrice ? startPrice.price.toString() : "unset"
    }, resolvedPrice=${
      resolvedPrice ? resolvedPrice.price.toString() : "unset"
    }`
  );
}

async function sendPythConsumerBatch(args: {
  receiver: {
    newTransactionBuilder: (config: { closeUpdateAccounts?: boolean }) => {
      addPostPriceUpdates: (priceUpdateDataArray: string[]) => Promise<void>;
      addPriceConsumerInstructions: (
        getInstructions: (
          getPriceUpdateAccount: (priceFeedId: string) => Web3PublicKey
        ) => Promise<
          Array<{
            instruction: TransactionInstruction;
            signers: Web3Signer[];
          }>
        >
      ) => Promise<void>;
      buildVersionedTransactions: (config: {
        computeUnitPriceMicroLamports: number;
        tightComputeBudget: boolean;
      }) => Promise<
        Array<{
          tx: VersionedTransaction;
          signers: Web3Signer[];
        }>
      >;
    };
    provider: AnchorProviderType;
  };
  feedIdHex: string;
  update: HermesPriceUpdate;
  buildInstructions: (priceUpdateAccount: Web3PublicKey) => Promise<
    Array<{
      instruction: TransactionInstruction;
      signers: Web3Signer[];
    }>
  >;
}) {
  const builder = args.receiver.newTransactionBuilder({
    closeUpdateAccounts: true,
  });

  await builder.addPostPriceUpdates(args.update.binary.data);
  await builder.addPriceConsumerInstructions(async (getPriceUpdateAccount) => {
    return args.buildInstructions(getPriceUpdateAccount(args.feedIdHex));
  });

  const transactions = await builder.buildVersionedTransactions({
    computeUnitPriceMicroLamports: Number(
      process.env.COMPUTE_UNIT_PRICE_MICROLAMPORTS ?? 1_000
    ),
    tightComputeBudget: true,
  });

  await args.receiver.provider.sendAll(transactions, {
    commitment: "confirmed",
  });
}

function ensurePythReceiverConfig(rpcUrl: string) {
  const isLocal =
    rpcUrl.includes("127.0.0.1") ||
    rpcUrl.includes("localhost") ||
    rpcUrl.includes("0.0.0.0");

  if (
    isLocal &&
    (!process.env.PYTH_RECEIVER_PROGRAM_ID ||
      !process.env.PYTH_WORMHOLE_PROGRAM_ID ||
      !process.env.PYTH_PUSH_ORACLE_PROGRAM_ID)
  ) {
    throw new Error(
      [
        "Local validator detected.",
        "This demo posts real Pyth updates through the Pyth Solana Receiver.",
        "For localnet, set PYTH_RECEIVER_PROGRAM_ID, PYTH_WORMHOLE_PROGRAM_ID, and PYTH_PUSH_ORACLE_PROGRAM_ID",
        "to your local deployments, or run the demo against a cluster where the Pyth receiver is already deployed.",
      ].join(" ")
    );
  }
}

async function main() {
  installRpcWebsocketsCompatShim();

  const { HermesClient } =
    require("@pythnetwork/hermes-client") as typeof import("@pythnetwork/hermes-client");
  const { PythSolanaReceiver } =
    require("@pythnetwork/pyth-solana-receiver") as typeof import("@pythnetwork/pyth-solana-receiver");

  const rpcUrl = process.env.ANCHOR_PROVIDER_URL ?? "http://127.0.0.1:8899";
  const walletPath = expandHome(
    process.env.ANCHOR_WALLET ??
      path.join(os.homedir(), ".config/solana/id.json")
  );
  const priceFeedIdHex =
    process.env.PYTH_PRICE_FEED_ID ?? DEFAULT_PRICE_FEED_ID;
  const collateralDecimals = readNumberEnv(
    "COLLATERAL_DECIMALS",
    DEFAULT_COLLATERAL_DECIMALS
  );
  const startDelaySeconds = readNumberEnv(
    "MARKET_START_DELAY_SECONDS",
    DEFAULT_START_DELAY_SECONDS
  );
  const endDelaySeconds = readNumberEnv(
    "MARKET_END_DELAY_SECONDS",
    DEFAULT_END_DELAY_SECONDS
  );

  if (endDelaySeconds <= startDelaySeconds) {
    throw new Error(
      "MARKET_END_DELAY_SECONDS must be greater than MARKET_START_DELAY_SECONDS."
    );
  }

  ensurePythReceiverConfig(rpcUrl);

  const connection = new Connection(rpcUrl, "confirmed");
  const traderKeypair = readKeypair(walletPath);
  const traderWallet = new Wallet(traderKeypair);
  const traderProvider = new AnchorProvider(
    connection,
    traderWallet,
    AnchorProvider.defaultOptions()
  );
  anchor.setProvider(traderProvider);

  const resolverKeypair = Keypair.generate();
  const resolverWallet = new Wallet(resolverKeypair);
  const resolverProvider = new AnchorProvider(
    connection,
    resolverWallet,
    AnchorProvider.defaultOptions()
  );

  const rawIdl = JSON.parse(
    fs.readFileSync(
      path.join(__dirname, "..", "target", "idl", "outcome_markets.json"),
      "utf8"
    )
  ) as anchor.Idl & { address: string };
  const programId = new PublicKey(
    process.env.OUTCOME_MARKETS_PROGRAM_ID ?? rawIdl.address
  );
  rawIdl.address = programId.toBase58();

  const traderProgram = new Program<OutcomeMarkets>(
    rawIdl as any,
    traderProvider
  );
  const resolverProgram = new Program<OutcomeMarkets>(
    rawIdl as any,
    resolverProvider
  );

  const hermes = new HermesClient(
    process.env.HERMES_URL ?? DEFAULT_HERMES_URL,
    {}
  );
  const pythReceiver = new PythSolanaReceiver({
    connection,
    wallet: resolverWallet,
    receiverProgramId: readOptionalPubkeyEnv("PYTH_RECEIVER_PROGRAM_ID"),
    wormholeProgramId: readOptionalPubkeyEnv("PYTH_WORMHOLE_PROGRAM_ID"),
    pushOracleProgramId: readOptionalPubkeyEnv("PYTH_PUSH_ORACLE_PROGRAM_ID"),
  });

  logDivider("Setup");
  console.log(`[config] rpc=${rpcUrl}`);
  console.log(`[config] wallet=${walletPath}`);
  console.log(`[config] outcome_markets_program_id=${programId.toBase58()}`);
  console.log(`[config] pyth_feed_id=${priceFeedIdHex}`);

  const resolverFundingTx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: traderWallet.publicKey,
      toPubkey: resolverWallet.publicKey,
      lamports: Math.floor(LAMPORTS_PER_SOL / 10),
    })
  );
  await traderProvider.sendAndConfirm(resolverFundingTx);
  console.log(
    `[setup] funded permissionless resolver ${resolverWallet.publicKey.toBase58()}`
  );

  const collateralMint = await createMint(
    connection,
    traderKeypair,
    traderWallet.publicKey,
    null,
    collateralDecimals
  );
  const traderCollateralAta = (
    await getOrCreateAssociatedTokenAccount(
      connection,
      traderKeypair,
      collateralMint,
      traderWallet.publicKey
    )
  ).address;

  const initialCollateral = rawTokens(10, collateralDecimals);
  await mintTo(
    connection,
    traderKeypair,
    collateralMint,
    traderCollateralAta,
    traderWallet.publicKey,
    initialCollateral
  );
  console.log(
    `[setup] minted ${formatUiAmount(
      initialCollateral,
      collateralDecimals
    )} demo collateral to ${traderCollateralAta.toBase58()}`
  );
  await logCollateralBalance(
    connection,
    "initial",
    traderCollateralAta,
    collateralDecimals
  );

  const latestPrice = await fetchLatestParsedPrice(hermes, priceFeedIdHex);
  const latestRawPrice = new AnchorBN(latestPrice.price.price, 10);
  const latestExponent = latestPrice.price.expo;
  if (latestRawPrice.lten(0)) {
    throw new Error(
      "This demo assumes a positive price feed so it can deterministically build YES and NO examples."
    );
  }
  console.log(
    `[oracle] latest price=${latestRawPrice.toString()} expo=${latestExponent} publish_time=${
      latestPrice.price.publish_time
    }`
  );

  const priceFeedIdBytes = toFeedIdBytes(priceFeedIdHex);
  const startTime = Math.floor(Date.now() / 1000) + startDelaySeconds;
  const endTime = Math.floor(Date.now() / 1000) + endDelaySeconds;
  const wideThreshold = latestRawPrice.muln(1_000);

  const marketBlueprints = [
    {
      label: "upDown",
      spec: { kind: "upDown" } as MarketSpec,
      splitAmount: rawTokens(3, collateralDecimals),
    },
    {
      label: "abovePriceNo",
      spec: {
        kind: "abovePrice",
        price: wideThreshold,
        exponent: latestExponent,
      } as MarketSpec,
      splitAmount: rawTokens(1, collateralDecimals),
    },
    {
      label: "belowPriceYes",
      spec: {
        kind: "belowPrice",
        price: wideThreshold,
        exponent: latestExponent,
      } as MarketSpec,
      splitAmount: rawTokens(1, collateralDecimals),
    },
    {
      label: "withinRangeYes",
      spec: {
        kind: "withinRange",
        lowerPrice: new AnchorBN(0),
        upperPrice: wideThreshold,
        exponent: latestExponent,
      } as MarketSpec,
      splitAmount: rawTokens(1, collateralDecimals),
    },
  ];

  logDivider("Initialize Markets");
  const markets: DemoMarket[] = [];
  for (const blueprint of marketBlueprints) {
    const pdas = deriveMarketPdas(
      programId,
      priceFeedIdBytes,
      startTime,
      endTime,
      blueprint.spec
    );

    const params = {
      priceFeedId: Array.from(priceFeedIdBytes),
      endTime: new AnchorBN(endTime),
      marketType: toAnchorMarketType(blueprint.spec),
      startTime: new AnchorBN(startTime),
    };

    await traderProgram.methods
      .initializeMarket(params as any)
      .accounts({
        payer: traderWallet.publicKey,
        market: pdas.market,
        yesMint: pdas.yesMint,
        noMint: pdas.noMint,
        collateralVault: pdas.collateralVault,
        collateralMint,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      } as any)
      .rpc();

    const yesAta = (
      await getOrCreateAssociatedTokenAccount(
        connection,
        traderKeypair,
        pdas.yesMint,
        traderWallet.publicKey
      )
    ).address;
    const noAta = (
      await getOrCreateAssociatedTokenAccount(
        connection,
        traderKeypair,
        pdas.noMint,
        traderWallet.publicKey
      )
    ).address;

    markets.push({
      ...blueprint,
      marketPda: pdas.market,
      yesMint: pdas.yesMint,
      noMint: pdas.noMint,
      collateralVault: pdas.collateralVault,
      yesAta,
      noAta,
    });

    console.log(
      `[init] ${
        blueprint.label
      }: market=${pdas.market.toBase58()} yesMint=${pdas.yesMint.toBase58()} noMint=${pdas.noMint.toBase58()}`
    );
  }

  logDivider("Split");
  for (const market of markets) {
    await traderProgram.methods
      .split(new AnchorBN(market.splitAmount))
      .accounts({
        user: traderWallet.publicKey,
        market: market.marketPda,
        collateralMint,
        yesMint: market.yesMint,
        noMint: market.noMint,
        collateralVault: market.collateralVault,
        userCollateralAccount: traderCollateralAta,
        userYesTokenAccount: market.yesAta,
        userNoTokenAccount: market.noAta,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      } as any)
      .rpc();

    console.log(
      `[split] ${market.label}: deposited ${formatUiAmount(
        market.splitAmount,
        collateralDecimals
      )}`
    );
  }
  await logCollateralBalance(
    connection,
    "after split",
    traderCollateralAta,
    collateralDecimals
  );

  const upDownMarket = markets.find((market) => market.label === "upDown");
  if (!upDownMarket) {
    throw new Error("UpDown demo market was not created.");
  }

  logDivider("Merge Before Resolution");
  const preResolutionMergeAmount = rawTokens(1, collateralDecimals);
  await traderProgram.methods
    .merge(new AnchorBN(preResolutionMergeAmount))
    .accounts({
      user: traderWallet.publicKey,
      market: upDownMarket.marketPda,
      collateralMint,
      yesMint: upDownMarket.yesMint,
      noMint: upDownMarket.noMint,
      collateralVault: upDownMarket.collateralVault,
      userCollateralAccount: traderCollateralAta,
      userYesTokenAccount: upDownMarket.yesAta,
      userNoTokenAccount: upDownMarket.noAta,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    } as any)
    .rpc();
  console.log(
    `[merge] upDown: merged ${formatUiAmount(
      preResolutionMergeAmount,
      collateralDecimals
    )} before resolution`
  );
  await logCollateralBalance(
    connection,
    "after pre-resolution merge",
    traderCollateralAta,
    collateralDecimals
  );

  logDivider("Set Start Price");
  await waitUntil(startTime + 1, "market start time");
  const startUpdate = await fetchBoundaryPriceUpdate(
    hermes,
    priceFeedIdHex,
    startTime,
    "start boundary"
  );
  await sendPythConsumerBatch({
    receiver: pythReceiver,
    feedIdHex: priceFeedIdHex,
    update: startUpdate,
    buildInstructions: async (priceUpdateAccount) => {
      return [
        {
          instruction: await resolverProgram.methods
            .setStartPrice()
            .accounts({
              updater: resolverWallet.publicKey,
              market: upDownMarket.marketPda,
              priceUpdate: priceUpdateAccount,
            } as any)
            .instruction(),
          signers: [],
        },
      ];
    },
  });
  console.log(
    `[setStartPrice] resolver ${resolverWallet.publicKey.toBase58()} permissionlessly set the UpDown start price`
  );
  await logMarketState(traderProgram, upDownMarket);

  logDivider("Resolve");
  await waitUntil(endTime + 1, "market end time");
  const endUpdate = await fetchBoundaryPriceUpdate(
    hermes,
    priceFeedIdHex,
    endTime,
    "end boundary"
  );
  await sendPythConsumerBatch({
    receiver: pythReceiver,
    feedIdHex: priceFeedIdHex,
    update: endUpdate,
    buildInstructions: async (priceUpdateAccount) => {
      return await Promise.all(
        markets.map(async (market) => ({
          instruction: await resolverProgram.methods
            .resolve()
            .accounts({
              resolver: resolverWallet.publicKey,
              market: market.marketPda,
              priceUpdate: priceUpdateAccount,
            } as any)
            .instruction(),
          signers: [],
        }))
      );
    },
  });
  console.log(
    `[resolve] resolver ${resolverWallet.publicKey.toBase58()} permissionlessly resolved all demo markets`
  );

  for (const market of markets) {
    await logMarketState(traderProgram, market);
  }

  logDivider("Claim");
  const postResolutionMergeAmount = rawTokens(1, collateralDecimals);

  const upDownAccount = (await traderProgram.account.outcomeMarket.fetch(
    upDownMarket.marketPda
  )) as Record<string, unknown>;
  const upDownResolution = enumVariant(upDownAccount.resolution);
  const upDownWinningMint =
    upDownResolution === "yes" ? upDownMarket.yesMint : upDownMarket.noMint;
  const upDownWinningAta =
    upDownResolution === "yes" ? upDownMarket.yesAta : upDownMarket.noAta;

  await traderProgram.methods
    .claim(new AnchorBN(postResolutionMergeAmount))
    .accounts({
      user: traderWallet.publicKey,
      market: upDownMarket.marketPda,
      collateralMint,
      collateralVault: upDownMarket.collateralVault,
      outcomeMint: upDownWinningMint,
      userCollateralAccount: traderCollateralAta,
      userOutcomeTokenAccount: upDownWinningAta,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    } as any)
    .rpc();
  console.log(
    `[claim] upDown: claimed ${formatUiAmount(
      postResolutionMergeAmount,
      collateralDecimals
    )} using only the winning-side token account (${upDownResolution.toUpperCase()})`
  );

  const abovePriceNoMarket = markets.find(
    (market) => market.label === "abovePriceNo"
  );
  if (!abovePriceNoMarket) {
    throw new Error("abovePriceNo demo market was not created.");
  }
  await traderProgram.methods
    .claim(new AnchorBN(abovePriceNoMarket.splitAmount))
    .accounts({
      user: traderWallet.publicKey,
      market: abovePriceNoMarket.marketPda,
      collateralMint,
      collateralVault: abovePriceNoMarket.collateralVault,
      outcomeMint: abovePriceNoMarket.noMint,
      userCollateralAccount: traderCollateralAta,
      userOutcomeTokenAccount: abovePriceNoMarket.noAta,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    } as any)
    .rpc();
  console.log(
    "[claim] abovePriceNo: claimed with the NO token account after a NO resolution"
  );

  const belowPriceYesMarket = markets.find(
    (market) => market.label === "belowPriceYes"
  );
  if (!belowPriceYesMarket) {
    throw new Error("belowPriceYes demo market was not created.");
  }
  await traderProgram.methods
    .claim(new AnchorBN(belowPriceYesMarket.splitAmount))
    .accounts({
      user: traderWallet.publicKey,
      market: belowPriceYesMarket.marketPda,
      collateralMint,
      collateralVault: belowPriceYesMarket.collateralVault,
      outcomeMint: belowPriceYesMarket.yesMint,
      userCollateralAccount: traderCollateralAta,
      userOutcomeTokenAccount: belowPriceYesMarket.yesAta,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    } as any)
    .rpc();
  console.log(
    "[claim] belowPriceYes: claimed with the YES token account after a YES resolution"
  );

  await logCollateralBalance(
    connection,
    "after claims",
    traderCollateralAta,
    collateralDecimals
  );

  logDivider("Merge After Resolution");
  await traderProgram.methods
    .merge(new AnchorBN(postResolutionMergeAmount))
    .accounts({
      user: traderWallet.publicKey,
      market: upDownMarket.marketPda,
      collateralMint,
      yesMint: upDownMarket.yesMint,
      noMint: upDownMarket.noMint,
      collateralVault: upDownMarket.collateralVault,
      userCollateralAccount: traderCollateralAta,
      userYesTokenAccount: upDownMarket.yesAta,
      userNoTokenAccount: upDownMarket.noAta,
      tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    } as any)
    .rpc();
  console.log(
    `[merge] upDown: merged ${formatUiAmount(
      postResolutionMergeAmount,
      collateralDecimals
    )} after resolution to demonstrate matched YES/NO pairs remain mergeable`
  );

  const upDownYesBalance = await fetchTokenAmount(
    connection,
    upDownMarket.yesAta
  );
  const upDownNoBalance = await fetchTokenAmount(
    connection,
    upDownMarket.noAta
  );
  await logCollateralBalance(
    connection,
    "final",
    traderCollateralAta,
    collateralDecimals
  );
  console.log(
    `[balance] upDown YES=${formatUiAmount(
      upDownYesBalance,
      collateralDecimals
    )} NO=${formatUiAmount(
      upDownNoBalance,
      collateralDecimals
    )} (one loser remains after claim + post-resolution merge)`
  );

  logDivider("Done");
  console.log("Demo completed successfully.");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
