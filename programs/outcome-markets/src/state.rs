use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::Price;

use crate::{constants::MARKET_TYPE_SEED_LEN, error::OutcomeMarketsError};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct InitializeMarketParams {
    pub price_feed_id: [u8; 32],
    pub end_time: i64,
    pub market_type: MarketType,
    pub start_time: i64,
}

impl InitializeMarketParams {
    pub fn validate(&self) -> Result<()> {
        require!(
            self.end_time > self.start_time,
            OutcomeMarketsError::InvalidTimeRange
        );
        self.market_type.validate()
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum MarketType {
    AbovePrice { price: i64, exponent: i32 },
    BelowPrice { price: i64, exponent: i32 },
    WithinRange {
        lower_price: i64,
        upper_price: i64,
        exponent: i32,
    },
    UpDown,
}

impl MarketType {
    pub fn seed_bytes(&self) -> [u8; MARKET_TYPE_SEED_LEN] {
        let mut bytes = [0u8; MARKET_TYPE_SEED_LEN];
        match self {
            Self::AbovePrice { price, exponent } => {
                bytes[0] = 0;
                bytes[1..5].copy_from_slice(&exponent.to_le_bytes());
                bytes[5..13].copy_from_slice(&price.to_le_bytes());
            }
            Self::BelowPrice { price, exponent } => {
                bytes[0] = 1;
                bytes[1..5].copy_from_slice(&exponent.to_le_bytes());
                bytes[5..13].copy_from_slice(&price.to_le_bytes());
            }
            Self::WithinRange {
                lower_price,
                upper_price,
                exponent,
            } => {
                bytes[0] = 2;
                bytes[1..5].copy_from_slice(&exponent.to_le_bytes());
                bytes[5..13].copy_from_slice(&lower_price.to_le_bytes());
                bytes[13..21].copy_from_slice(&upper_price.to_le_bytes());
            }
            Self::UpDown => {
                bytes[0] = 3;
            }
        }
        bytes
    }

    pub fn validate(&self) -> Result<()> {
        if let Self::WithinRange {
            lower_price,
            upper_price,
            ..
        } = self
        {
            require!(
                lower_price <= upper_price,
                OutcomeMarketsError::InvalidPriceBounds
            );
        }

        Ok(())
    }

    pub fn supports_start_price(&self) -> bool {
        matches!(self, Self::UpDown)
    }

    pub fn resolve_outcome(
        &self,
        start_price: Option<&RecordedPrice>,
        end_price: &RecordedPrice,
    ) -> Result<Resolution> {
        match self {
            Self::AbovePrice { price, exponent } => {
                require_eq!(
                    end_price.exponent,
                    *exponent,
                    OutcomeMarketsError::UnexpectedPriceExponent
                );
                Ok(if end_price.price > *price {
                    Resolution::Yes
                } else {
                    Resolution::No
                })
            }
            Self::BelowPrice { price, exponent } => {
                require_eq!(
                    end_price.exponent,
                    *exponent,
                    OutcomeMarketsError::UnexpectedPriceExponent
                );
                Ok(if end_price.price < *price {
                    Resolution::Yes
                } else {
                    Resolution::No
                })
            }
            Self::WithinRange {
                lower_price,
                upper_price,
                exponent,
            } => {
                require_eq!(
                    end_price.exponent,
                    *exponent,
                    OutcomeMarketsError::UnexpectedPriceExponent
                );
                Ok(if end_price.price >= *lower_price && end_price.price <= *upper_price {
                    Resolution::Yes
                } else {
                    Resolution::No
                })
            }
            Self::UpDown => {
                let start_price =
                    start_price.ok_or_else(|| error!(OutcomeMarketsError::StartPriceNotSet))?;
                require_eq!(
                    start_price.exponent,
                    end_price.exponent,
                    OutcomeMarketsError::UnexpectedPriceExponent
                );
                Ok(if end_price.price > start_price.price {
                    Resolution::Yes
                } else {
                    Resolution::No
                })
            }
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Unresolved,
    Yes,
    No,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedPrice {
    pub price: i64,
    pub exponent: i32,
    pub publish_time: i64,
}

impl RecordedPrice {
    pub fn from_pyth(price: Price) -> Self {
        Self {
            price: price.price,
            exponent: price.exponent,
            publish_time: price.publish_time,
        }
    }
}

#[account]
pub struct OutcomeMarket {
    pub creator: Pubkey,
    pub price_feed_id: [u8; 32],
    pub market_type_seed: [u8; MARKET_TYPE_SEED_LEN],
    pub market_type: MarketType,
    pub collateral_mint: Pubkey,
    pub yes_mint: Pubkey,
    pub no_mint: Pubkey,
    pub collateral_vault: Pubkey,
    pub start_time: i64,
    pub end_time: i64,
    pub start_price: Option<RecordedPrice>,
    pub resolved_price: Option<RecordedPrice>,
    pub resolution: Resolution,
    pub bump: u8,
    pub yes_mint_bump: u8,
    pub no_mint_bump: u8,
    pub collateral_vault_bump: u8,
}

impl OutcomeMarket {
    pub const LEN: usize = 8 + 297;

    pub fn is_resolved(&self) -> bool {
        self.resolution != Resolution::Unresolved
    }
}
