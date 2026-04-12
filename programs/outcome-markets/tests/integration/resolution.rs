use anchor_lang::AccountDeserialize;
use litesvm::LiteSVM;
use outcome_markets::state::{InitializeMarketParams, MarketType, OutcomeMarket, Resolution};
use solana_keypair::{Keypair as SKeypair, Signer as SSigner};
use solana_sdk::{program_pack::Pack, pubkey::Pubkey, signature::Keypair, signer::Signer};
use spl_token::state::Account as TokenAccount;

use crate::helpers::{
    account::load_account,
    clock::set_unix_timestamp,
    market::{
        claim_ix, initialize_market_ix, resolve_ix, set_start_price_ix, split_ix, ONE_USDC,
        USDC_MINT,
    },
    oracle::create_price_update_account,
    program::load_outcome_markets_program,
    token::create_token_account,
    transaction::prepare_v0_tx,
};

const FEED_ID: [u8; 32] = [9u8; 32];

#[test]
fn updown_market_sets_start_price_resolves_yes_and_claims() {
    let svm_user = SKeypair::new();
    let user = Keypair::from_bytes(&svm_user.to_bytes()).unwrap();

    let mut svm = LiteSVM::new();
    load_outcome_markets_program(&mut svm);

    svm.airdrop(&user.pubkey().to_bytes().into(), 1_000_000_000)
        .unwrap();
    load_account(&mut svm, &USDC_MINT);

    let user_collateral_account =
        create_token_account(&mut svm, &user.pubkey(), &USDC_MINT, 3 * ONE_USDC);
    let params = InitializeMarketParams {
        price_feed_id: FEED_ID,
        end_time: 100,
        market_type: MarketType::UpDown,
        start_time: 10,
    };

    let (initialize_ix, market, yes_mint, no_mint, collateral_vault) =
        initialize_market_ix(user.pubkey(), USDC_MINT, params);
    let init_tx = prepare_v0_tx(&mut svm, &svm_user.pubkey(), &[&svm_user], &[], &[initialize_ix]);
    svm.send_transaction(init_tx).unwrap();

    let user_yes_token_account = create_token_account(&mut svm, &user.pubkey(), &yes_mint, 0);
    let user_no_token_account = create_token_account(&mut svm, &user.pubkey(), &no_mint, 0);

    let split_instruction = split_ix(
        user.pubkey(),
        market,
        USDC_MINT,
        yes_mint,
        no_mint,
        collateral_vault,
        user_collateral_account,
        user_yes_token_account,
        user_no_token_account,
        ONE_USDC,
    );
    let split_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[split_instruction],
    );
    svm.send_transaction(split_tx).unwrap();

    let start_price_update = Pubkey::new_unique();
    create_price_update_account(&mut svm, &start_price_update, FEED_ID, 100_000_000, -8, 12);
    let set_start_price_instruction = set_start_price_ix(user.pubkey(), market, start_price_update);
    set_unix_timestamp(&mut svm, 12);
    let set_start_price_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[set_start_price_instruction],
    );
    svm.send_transaction(set_start_price_tx).unwrap();

    let resolve_price_update = Pubkey::new_unique();
    create_price_update_account(&mut svm, &resolve_price_update, FEED_ID, 125_000_000, -8, 101);
    let resolve_instruction = resolve_ix(user.pubkey(), market, resolve_price_update);
    set_unix_timestamp(&mut svm, 101);
    let resolve_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[resolve_instruction],
    );
    svm.send_transaction(resolve_tx).unwrap();

    let market_account = read_market(&svm, &market);
    assert_eq!(market_account.resolution, Resolution::Yes);
    assert_eq!(market_account.start_price.unwrap().price, 100_000_000);
    assert_eq!(market_account.resolved_price.unwrap().price, 125_000_000);

    let claim_instruction = claim_ix(
        user.pubkey(),
        market,
        USDC_MINT,
        collateral_vault,
        yes_mint,
        user_collateral_account,
        user_yes_token_account,
        ONE_USDC,
    );
    let claim_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[claim_instruction],
    );
    svm.send_transaction(claim_tx).unwrap();

    assert_eq!(token_balance(&svm, &user_collateral_account), 3 * ONE_USDC);
    assert_eq!(token_balance(&svm, &collateral_vault), 0);
    assert_eq!(token_balance(&svm, &user_yes_token_account), 0);
    assert_eq!(token_balance(&svm, &user_no_token_account), ONE_USDC);
}

#[test]
fn within_range_resolution_is_inclusive() {
    let svm_user = SKeypair::new();
    let user = Keypair::from_bytes(&svm_user.to_bytes()).unwrap();

    let mut svm = LiteSVM::new();
    load_outcome_markets_program(&mut svm);

    svm.airdrop(&user.pubkey().to_bytes().into(), 1_000_000_000)
        .unwrap();
    load_account(&mut svm, &USDC_MINT);

    let user_collateral_account =
        create_token_account(&mut svm, &user.pubkey(), &USDC_MINT, 2 * ONE_USDC);
    let params = InitializeMarketParams {
        price_feed_id: FEED_ID,
        end_time: 100,
        market_type: MarketType::WithinRange {
            lower_price: 90_000_000,
            upper_price: 100_000_000,
            exponent: -8,
        },
        start_time: 10,
    };

    let (initialize_ix, market, yes_mint, no_mint, collateral_vault) =
        initialize_market_ix(user.pubkey(), USDC_MINT, params);
    let init_tx = prepare_v0_tx(&mut svm, &svm_user.pubkey(), &[&svm_user], &[], &[initialize_ix]);
    svm.send_transaction(init_tx).unwrap();

    let user_yes_token_account = create_token_account(&mut svm, &user.pubkey(), &yes_mint, 0);
    let user_no_token_account = create_token_account(&mut svm, &user.pubkey(), &no_mint, 0);

    let split_instruction = split_ix(
        user.pubkey(),
        market,
        USDC_MINT,
        yes_mint,
        no_mint,
        collateral_vault,
        user_collateral_account,
        user_yes_token_account,
        user_no_token_account,
        ONE_USDC,
    );
    let split_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[split_instruction],
    );
    svm.send_transaction(split_tx).unwrap();

    let resolve_price_update = Pubkey::new_unique();
    create_price_update_account(&mut svm, &resolve_price_update, FEED_ID, 100_000_000, -8, 101);
    let resolve_instruction = resolve_ix(user.pubkey(), market, resolve_price_update);
    set_unix_timestamp(&mut svm, 101);
    let resolve_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[resolve_instruction],
    );
    svm.send_transaction(resolve_tx).unwrap();

    let market_account = read_market(&svm, &market);
    assert_eq!(market_account.resolution, Resolution::Yes);
}

#[test]
fn set_start_price_requires_clock_to_reach_market_start_time() {
    let svm_user = SKeypair::new();
    let user = Keypair::from_bytes(&svm_user.to_bytes()).unwrap();

    let mut svm = LiteSVM::new();
    load_outcome_markets_program(&mut svm);

    svm.airdrop(&user.pubkey().to_bytes().into(), 1_000_000_000)
        .unwrap();
    load_account(&mut svm, &USDC_MINT);

    let params = InitializeMarketParams {
        price_feed_id: FEED_ID,
        end_time: 100,
        market_type: MarketType::UpDown,
        start_time: 10,
    };

    let (initialize_ix, market, _yes_mint, _no_mint, _collateral_vault) =
        initialize_market_ix(user.pubkey(), USDC_MINT, params);
    let init_tx = prepare_v0_tx(&mut svm, &svm_user.pubkey(), &[&svm_user], &[], &[initialize_ix]);
    svm.send_transaction(init_tx).unwrap();

    let start_price_update = Pubkey::new_unique();
    create_price_update_account(&mut svm, &start_price_update, FEED_ID, 100_000_000, -8, 10);
    let set_start_price_instruction = set_start_price_ix(user.pubkey(), market, start_price_update);

    set_unix_timestamp(&mut svm, 9);
    let early_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[set_start_price_instruction.clone()],
    );
    assert!(svm.send_transaction(early_tx).is_err());

    set_unix_timestamp(&mut svm, 10);
    svm.expire_blockhash();
    let on_time_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[set_start_price_instruction],
    );
    svm.send_transaction(on_time_tx).unwrap();
}

#[test]
fn resolve_requires_clock_to_reach_market_end_time() {
    let svm_user = SKeypair::new();
    let user = Keypair::from_bytes(&svm_user.to_bytes()).unwrap();

    let mut svm = LiteSVM::new();
    load_outcome_markets_program(&mut svm);

    svm.airdrop(&user.pubkey().to_bytes().into(), 1_000_000_000)
        .unwrap();
    load_account(&mut svm, &USDC_MINT);

    let params = InitializeMarketParams {
        price_feed_id: FEED_ID,
        end_time: 100,
        market_type: MarketType::AbovePrice {
            price: 100_000_000,
            exponent: -8,
        },
        start_time: 10,
    };

    let (initialize_ix, market, _yes_mint, _no_mint, _collateral_vault) =
        initialize_market_ix(user.pubkey(), USDC_MINT, params);
    let init_tx = prepare_v0_tx(&mut svm, &svm_user.pubkey(), &[&svm_user], &[], &[initialize_ix]);
    svm.send_transaction(init_tx).unwrap();

    let resolve_price_update = Pubkey::new_unique();
    create_price_update_account(&mut svm, &resolve_price_update, FEED_ID, 125_000_000, -8, 100);
    let resolve_instruction = resolve_ix(user.pubkey(), market, resolve_price_update);

    set_unix_timestamp(&mut svm, 99);
    let early_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[resolve_instruction.clone()],
    );
    assert!(svm.send_transaction(early_tx).is_err());

    set_unix_timestamp(&mut svm, 100);
    svm.expire_blockhash();
    let on_time_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[resolve_instruction],
    );
    svm.send_transaction(on_time_tx).unwrap();
}

fn read_market(svm: &LiteSVM, market: &Pubkey) -> OutcomeMarket {
    let account = svm.get_account(&market.to_bytes().into()).unwrap();
    let mut data = account.data.as_slice();
    OutcomeMarket::try_deserialize(&mut data).unwrap()
}

fn token_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let account = svm.get_account(&account.to_bytes().into()).unwrap();
    let token_account = TokenAccount::unpack_from_slice(&account.data).unwrap();
    token_account.amount
}
