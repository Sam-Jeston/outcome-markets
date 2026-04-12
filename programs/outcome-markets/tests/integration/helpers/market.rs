use anchor_lang::{InstructionData, ToAccountMetas};
use outcome_markets::{
    constants::{COLLATERAL_VAULT_SEED, MARKET_SEED, NO_MINT_SEED, YES_MINT_SEED},
    state::InitializeMarketParams,
};
use solana_message::{AccountMeta, Instruction};
use solana_sdk::{pubkey::Pubkey, system_program, sysvar};

pub const USDC_MINT: Pubkey = solana_sdk::pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
pub const ONE_USDC: u64 = 1_000_000;

pub fn market_pdas(
    params: &InitializeMarketParams,
) -> (Pubkey, Pubkey, Pubkey, Pubkey) {
    let market = Pubkey::find_program_address(
        &[
            MARKET_SEED,
            params.price_feed_id.as_ref(),
            &params.end_time.to_le_bytes(),
            params.market_type.seed_bytes().as_ref(),
            &params.start_time.to_le_bytes(),
        ],
        &outcome_markets::id(),
    )
    .0;
    let yes_mint =
        Pubkey::find_program_address(&[market.as_ref(), YES_MINT_SEED], &outcome_markets::id()).0;
    let no_mint =
        Pubkey::find_program_address(&[market.as_ref(), NO_MINT_SEED], &outcome_markets::id()).0;
    let collateral_vault = Pubkey::find_program_address(
        &[market.as_ref(), COLLATERAL_VAULT_SEED],
        &outcome_markets::id(),
    )
    .0;

    (market, yes_mint, no_mint, collateral_vault)
}

pub fn initialize_market_ix(
    payer: Pubkey,
    collateral_mint: Pubkey,
    params: InitializeMarketParams,
) -> (Instruction, Pubkey, Pubkey, Pubkey, Pubkey) {
    let (market, yes_mint, no_mint, collateral_vault) = market_pdas(&params);

    let instruction = Instruction {
        program_id: outcome_markets::id().to_bytes().into(),
        accounts: outcome_markets::accounts::InitializeMarket {
            payer,
            market,
            yes_mint,
            no_mint,
            collateral_vault,
            collateral_mint,
            token_program: spl_token::ID,
            system_program: system_program::ID,
            rent: sysvar::rent::ID,
        }
        .to_account_metas(None)
        .into_iter()
        .map(|meta| AccountMeta {
            pubkey: meta.pubkey.to_bytes().into(),
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        })
        .collect(),
        data: outcome_markets::instruction::InitializeMarket { params }.data(),
    };

    (instruction, market, yes_mint, no_mint, collateral_vault)
}

pub fn split_ix(
    user: Pubkey,
    market: Pubkey,
    collateral_mint: Pubkey,
    yes_mint: Pubkey,
    no_mint: Pubkey,
    collateral_vault: Pubkey,
    user_collateral_account: Pubkey,
    user_yes_token_account: Pubkey,
    user_no_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction {
        program_id: outcome_markets::id().to_bytes().into(),
        accounts: outcome_markets::accounts::Split {
            user,
            market,
            collateral_mint,
            yes_mint,
            no_mint,
            collateral_vault,
            user_collateral_account,
            user_yes_token_account,
            user_no_token_account,
            token_program: spl_token::ID,
        }
        .to_account_metas(None)
        .into_iter()
        .map(|meta| AccountMeta {
            pubkey: meta.pubkey.to_bytes().into(),
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        })
        .collect(),
        data: outcome_markets::instruction::Split { amount }.data(),
    }
}

pub fn merge_ix(
    user: Pubkey,
    market: Pubkey,
    collateral_mint: Pubkey,
    yes_mint: Pubkey,
    no_mint: Pubkey,
    collateral_vault: Pubkey,
    user_collateral_account: Pubkey,
    user_yes_token_account: Pubkey,
    user_no_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction {
        program_id: outcome_markets::id().to_bytes().into(),
        accounts: outcome_markets::accounts::Merge {
            user,
            market,
            collateral_mint,
            yes_mint,
            no_mint,
            collateral_vault,
            user_collateral_account,
            user_yes_token_account,
            user_no_token_account,
            token_program: spl_token::ID,
        }
        .to_account_metas(None)
        .into_iter()
        .map(|meta| AccountMeta {
            pubkey: meta.pubkey.to_bytes().into(),
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        })
        .collect(),
        data: outcome_markets::instruction::Merge { amount }.data(),
    }
}

pub fn set_start_price_ix(user: Pubkey, market: Pubkey, price_update: Pubkey) -> Instruction {
    Instruction {
        program_id: outcome_markets::id().to_bytes().into(),
        accounts: outcome_markets::accounts::SetStartPrice {
            updater: user,
            market,
            price_update,
        }
        .to_account_metas(None)
        .into_iter()
        .map(|meta| AccountMeta {
            pubkey: meta.pubkey.to_bytes().into(),
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        })
        .collect(),
        data: outcome_markets::instruction::SetStartPrice {}.data(),
    }
}

pub fn resolve_ix(user: Pubkey, market: Pubkey, price_update: Pubkey) -> Instruction {
    Instruction {
        program_id: outcome_markets::id().to_bytes().into(),
        accounts: outcome_markets::accounts::Resolve {
            resolver: user,
            market,
            price_update,
        }
        .to_account_metas(None)
        .into_iter()
        .map(|meta| AccountMeta {
            pubkey: meta.pubkey.to_bytes().into(),
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        })
        .collect(),
        data: outcome_markets::instruction::Resolve {}.data(),
    }
}

pub fn claim_ix(
    user: Pubkey,
    market: Pubkey,
    collateral_mint: Pubkey,
    yes_mint: Pubkey,
    no_mint: Pubkey,
    collateral_vault: Pubkey,
    user_collateral_account: Pubkey,
    user_yes_token_account: Pubkey,
    user_no_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction {
        program_id: outcome_markets::id().to_bytes().into(),
        accounts: outcome_markets::accounts::Claim {
            user,
            market,
            collateral_mint,
            yes_mint,
            no_mint,
            collateral_vault,
            user_collateral_account,
            user_yes_token_account,
            user_no_token_account,
            token_program: spl_token::ID,
        }
        .to_account_metas(None)
        .into_iter()
        .map(|meta| AccountMeta {
            pubkey: meta.pubkey.to_bytes().into(),
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        })
        .collect(),
        data: outcome_markets::instruction::Claim { amount }.data(),
    }
}
