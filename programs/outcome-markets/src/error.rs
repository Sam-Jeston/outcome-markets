use anchor_lang::prelude::*;

#[error_code]
pub enum OutcomeMarketsError {
    #[msg("The market timing parameters are invalid.")]
    InvalidTimeRange,
    #[msg("The amount must be greater than zero.")]
    InvalidAmount,
    #[msg("The collateral mint must use 6 decimals.")]
    InvalidCollateralMintDecimals,
    #[msg("This market has already ended.")]
    MarketAlreadyEnded,
    #[msg("This market has already been resolved.")]
    MarketAlreadyResolved,
    #[msg("This market has not been resolved.")]
    MarketNotResolved,
    #[msg("The market is no longer open for splitting.")]
    MarketClosedForSplits,
    #[msg("The start price has already been set.")]
    StartPriceAlreadySet,
    #[msg("The start price has not been set.")]
    StartPriceNotSet,
    #[msg("This market type does not use a start price.")]
    StartPriceNotSupported,
    #[msg("The provided oracle update is before the required market timestamp.")]
    OracleUpdateTooEarly,
    #[msg("The provided oracle update is invalid for this market.")]
    InvalidPriceUpdate,
    #[msg("The provided oracle update does not have full verification.")]
    InsufficientPythVerification,
    #[msg("The oracle price exponent does not match the market configuration.")]
    UnexpectedPriceExponent,
    #[msg("The provided collateral mint does not match the market.")]
    InvalidCollateralMint,
    #[msg("The provided YES mint does not match the market.")]
    InvalidYesMint,
    #[msg("The provided NO mint does not match the market.")]
    InvalidNoMint,
    #[msg("The provided collateral vault does not match the market.")]
    InvalidCollateralVault,
    #[msg("The provided collateral token account is invalid.")]
    InvalidCollateralAccount,
    #[msg("The provided YES token account is invalid.")]
    InvalidYesTokenAccount,
    #[msg("The provided NO token account is invalid.")]
    InvalidNoTokenAccount,
    #[msg("The token account owner is invalid.")]
    InvalidTokenOwner,
    #[msg("The price bounds for this market type are invalid.")]
    InvalidPriceBounds,
}
