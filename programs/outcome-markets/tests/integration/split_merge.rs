use anchor_lang::AccountDeserialize;
use litesvm::LiteSVM;
use outcome_markets::state::{InitializeMarketParams, MarketType, OutcomeMarket, Resolution};
use solana_keypair::{Keypair as SKeypair, Signer as SSigner};
use solana_sdk::{program_pack::Pack, pubkey::Pubkey, signature::Keypair, signer::Signer};
use spl_token::state::Account as TokenAccount;

use crate::helpers::{
    account::load_account,
    market::{initialize_market_ix, merge_ix, split_ix, ONE_USDC, USDC_MINT},
    program::load_outcome_markets_program,
    token::create_token_account,
    transaction::prepare_v0_tx,
};

const FEED_ID: [u8; 32] = [7u8; 32];

#[test]
fn initialize_split_and_merge_round_trip_collateral() {
    let svm_user = SKeypair::new();
    let user = Keypair::from_bytes(&svm_user.to_bytes()).unwrap();

    let mut svm = LiteSVM::new();
    load_outcome_markets_program(&mut svm);

    svm.airdrop(&user.pubkey().to_bytes().into(), 1_000_000_000)
        .unwrap();
    load_account(&mut svm, &USDC_MINT);

    let user_collateral_account =
        create_token_account(&mut svm, &user.pubkey(), &USDC_MINT, 5 * ONE_USDC);
    let params = InitializeMarketParams {
        price_feed_id: FEED_ID,
        end_time: 100,
        market_type: MarketType::AbovePrice {
            price: 100_000_000,
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

    assert_eq!(
        token_balance(&svm, &user_collateral_account),
        4 * ONE_USDC
    );
    assert_eq!(token_balance(&svm, &collateral_vault), ONE_USDC);
    assert_eq!(token_balance(&svm, &user_yes_token_account), ONE_USDC);
    assert_eq!(token_balance(&svm, &user_no_token_account), ONE_USDC);

    let merge_instruction = merge_ix(
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
    let merge_tx = prepare_v0_tx(
        &mut svm,
        &svm_user.pubkey(),
        &[&svm_user],
        &[],
        &[merge_instruction],
    );
    svm.send_transaction(merge_tx).unwrap();

    assert_eq!(
        token_balance(&svm, &user_collateral_account),
        5 * ONE_USDC
    );
    assert_eq!(token_balance(&svm, &collateral_vault), 0);
    assert_eq!(token_balance(&svm, &user_yes_token_account), 0);
    assert_eq!(token_balance(&svm, &user_no_token_account), 0);

    let market_account = read_market(&svm, &market);
    assert_eq!(market_account.resolution, Resolution::Unresolved);
    assert_eq!(market_account.yes_mint, yes_mint);
    assert_eq!(market_account.no_mint, no_mint);
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
