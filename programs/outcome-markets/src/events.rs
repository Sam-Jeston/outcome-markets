use anchor_lang::prelude::*;

use crate::state::{MarketType, RecordedPrice, Resolution};

#[event]
pub struct MarketInitializedEvent {
    pub market: Pubkey,
    pub creator: Pubkey,
    pub collateral_mint: Pubkey,
    pub yes_mint: Pubkey,
    pub no_mint: Pubkey,
    pub collateral_vault: Pubkey,
    pub price_feed_id: [u8; 32],
    pub market_type: MarketType,
    pub start_time: i64,
    pub end_time: i64,
    pub market_bump: u8,
    pub yes_mint_bump: u8,
    pub no_mint_bump: u8,
    pub collateral_vault_bump: u8,
    pub emitted_at: i64,
}

#[event]
pub struct MarketSplitEvent {
    pub market: Pubkey,
    pub user: Pubkey,
    pub collateral_mint: Pubkey,
    pub yes_mint: Pubkey,
    pub no_mint: Pubkey,
    pub collateral_vault: Pubkey,
    pub user_collateral_account: Pubkey,
    pub user_yes_token_account: Pubkey,
    pub user_no_token_account: Pubkey,
    pub amount: u64,
    pub emitted_at: i64,
}

#[event]
pub struct MarketMergedEvent {
    pub market: Pubkey,
    pub user: Pubkey,
    pub collateral_mint: Pubkey,
    pub yes_mint: Pubkey,
    pub no_mint: Pubkey,
    pub collateral_vault: Pubkey,
    pub user_collateral_account: Pubkey,
    pub user_yes_token_account: Pubkey,
    pub user_no_token_account: Pubkey,
    pub amount: u64,
    pub emitted_at: i64,
}

#[event]
pub struct MarketStartPriceSetEvent {
    pub market: Pubkey,
    pub updater: Pubkey,
    pub price_feed_id: [u8; 32],
    pub start_time: i64,
    pub start_price: RecordedPrice,
    pub emitted_at: i64,
}

#[event]
pub struct MarketResolvedEvent {
    pub market: Pubkey,
    pub resolver: Pubkey,
    pub price_feed_id: [u8; 32],
    pub market_type: MarketType,
    pub start_price: Option<RecordedPrice>,
    pub resolved_price: RecordedPrice,
    pub resolution: Resolution,
    pub end_time: i64,
    pub emitted_at: i64,
}

#[event]
pub struct MarketMetadataInitializedEvent {
    pub market: Pubkey,
    pub mint: Pubkey,
    pub metadata: Pubkey,
    pub side: OutcomeSide,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub emitted_at: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeSide {
    Yes,
    No,
}

#[event]
pub struct MarketClaimedEvent {
    pub market: Pubkey,
    pub user: Pubkey,
    pub collateral_mint: Pubkey,
    pub collateral_vault: Pubkey,
    pub outcome_mint: Pubkey,
    pub user_collateral_account: Pubkey,
    pub user_outcome_token_account: Pubkey,
    pub resolution: Resolution,
    pub amount: u64,
    pub emitted_at: i64,
}
