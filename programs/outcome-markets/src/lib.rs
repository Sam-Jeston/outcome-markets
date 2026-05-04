#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_spl::metadata::{
    create_metadata_accounts_v3,
    mpl_token_metadata::types::DataV2,
    CreateMetadataAccountsV3, Metadata,
};
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, TransferChecked};
use pyth_solana_receiver_sdk::price_update::{Price, PriceUpdateV2, VerificationLevel};

pub mod constants;
pub mod error;
pub mod events;
pub mod state;

use constants::{
    COLLATERAL_VAULT_SEED, MARKET_SEED, METADATA_NAME_MAX_LEN, METADATA_SYMBOL_MAX_LEN,
    METADATA_URI_MAX_LEN, NO_MINT_SEED, YES_MINT_SEED,
};
use error::OutcomeMarketsError;
use events::{
    MarketClaimedEvent, MarketInitializedEvent, MarketMergedEvent,
    MarketMetadataInitializedEvent, MarketResolvedEvent, MarketSplitEvent,
    MarketStartPriceSetEvent, OutcomeSide,
};
use state::{InitializeMarketParams, OutcomeMarket, RecordedPrice, Resolution};

declare_id!("23uBqw2FZEUAj5JtTuzCHidyijuNZQmqvMTDPAjXJp6U");

#[program]
pub mod outcome_markets {
    use super::*;

    pub fn initialize_market(
        ctx: Context<InitializeMarket>,
        params: InitializeMarketParams,
    ) -> Result<()> {
        let clock = Clock::get()?;
        let market_key = ctx.accounts.market.key();
        params.validate()?;

        require!(
            clock.unix_timestamp < params.end_time,
            OutcomeMarketsError::MarketAlreadyEnded
        );

        let market = &mut ctx.accounts.market;
        market.creator = ctx.accounts.payer.key();
        market.price_feed_id = params.price_feed_id;
        market.market_type_seed = params.market_type.seed_bytes();
        market.market_type = params.market_type;
        market.collateral_mint = ctx.accounts.collateral_mint.key();
        market.yes_mint = ctx.accounts.yes_mint.key();
        market.no_mint = ctx.accounts.no_mint.key();
        market.collateral_vault = ctx.accounts.collateral_vault.key();
        market.start_time = params.start_time;
        market.end_time = params.end_time;
        market.start_price = None;
        market.resolved_price = None;
        market.resolution = Resolution::Unresolved;
        market.bump = ctx.bumps.market;
        market.yes_mint_bump = ctx.bumps.yes_mint;
        market.no_mint_bump = ctx.bumps.no_mint;
        market.collateral_vault_bump = ctx.bumps.collateral_vault;

        emit!(MarketInitializedEvent {
            market: market_key,
            creator: market.creator,
            collateral_mint: market.collateral_mint,
            yes_mint: market.yes_mint,
            no_mint: market.no_mint,
            collateral_vault: market.collateral_vault,
            price_feed_id: market.price_feed_id,
            market_type: market.market_type.clone(),
            start_time: market.start_time,
            end_time: market.end_time,
            market_bump: market.bump,
            yes_mint_bump: market.yes_mint_bump,
            no_mint_bump: market.no_mint_bump,
            collateral_vault_bump: market.collateral_vault_bump,
            emitted_at: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn split(ctx: Context<Split>, amount: u64) -> Result<()> {
        let clock = Clock::get()?;
        require!(amount > 0, OutcomeMarketsError::InvalidAmount);
        require!(
            !ctx.accounts.market.is_resolved(),
            OutcomeMarketsError::MarketAlreadyResolved
        );
        require!(
            clock.unix_timestamp < ctx.accounts.market.end_time,
            OutcomeMarketsError::MarketClosedForSplits
        );

        token::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.user_collateral_account.to_account_info(),
                    mint: ctx.accounts.collateral_mint.to_account_info(),
                    to: ctx.accounts.collateral_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
            ctx.accounts.collateral_mint.decimals,
        )?;

        let market = &ctx.accounts.market;
        let end_time_bytes = market.end_time.to_le_bytes();
        let start_time_bytes = market.start_time.to_le_bytes();
        let signer_seeds = &[
            MARKET_SEED,
            market.price_feed_id.as_ref(),
            end_time_bytes.as_ref(),
            market.market_type_seed.as_ref(),
            start_time_bytes.as_ref(),
            &[market.bump],
        ];

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.yes_mint.to_account_info(),
                    to: ctx.accounts.user_yes_token_account.to_account_info(),
                    authority: ctx.accounts.market.to_account_info(),
                },
                &[signer_seeds],
            ),
            amount,
        )?;

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.no_mint.to_account_info(),
                    to: ctx.accounts.user_no_token_account.to_account_info(),
                    authority: ctx.accounts.market.to_account_info(),
                },
                &[signer_seeds],
            ),
            amount,
        )?;

        emit!(MarketSplitEvent {
            market: ctx.accounts.market.key(),
            user: ctx.accounts.user.key(),
            collateral_mint: ctx.accounts.collateral_mint.key(),
            yes_mint: ctx.accounts.yes_mint.key(),
            no_mint: ctx.accounts.no_mint.key(),
            collateral_vault: ctx.accounts.collateral_vault.key(),
            user_collateral_account: ctx.accounts.user_collateral_account.key(),
            user_yes_token_account: ctx.accounts.user_yes_token_account.key(),
            user_no_token_account: ctx.accounts.user_no_token_account.key(),
            amount,
            emitted_at: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn merge(ctx: Context<Merge>, amount: u64) -> Result<()> {
        require!(amount > 0, OutcomeMarketsError::InvalidAmount);

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.yes_mint.to_account_info(),
                    from: ctx.accounts.user_yes_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.no_mint.to_account_info(),
                    from: ctx.accounts.user_no_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        transfer_market_collateral(
            ctx.accounts.market.as_ref(),
            &ctx.accounts.token_program,
            ctx.accounts.collateral_vault.as_ref(),
            ctx.accounts.collateral_mint.as_ref(),
            ctx.accounts.user_collateral_account.as_ref(),
            amount,
        )?;

        emit!(MarketMergedEvent {
            market: ctx.accounts.market.key(),
            user: ctx.accounts.user.key(),
            collateral_mint: ctx.accounts.collateral_mint.key(),
            yes_mint: ctx.accounts.yes_mint.key(),
            no_mint: ctx.accounts.no_mint.key(),
            collateral_vault: ctx.accounts.collateral_vault.key(),
            user_collateral_account: ctx.accounts.user_collateral_account.key(),
            user_yes_token_account: ctx.accounts.user_yes_token_account.key(),
            user_no_token_account: ctx.accounts.user_no_token_account.key(),
            amount,
            emitted_at: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn set_start_price(ctx: Context<SetStartPrice>) -> Result<()> {
        let market_key = ctx.accounts.market.key();
        let market = &mut ctx.accounts.market;
        let clock = Clock::get()?;

        require!(
            market.market_type.supports_start_price(),
            OutcomeMarketsError::StartPriceNotSupported
        );
        require!(
            !market.is_resolved(),
            OutcomeMarketsError::MarketAlreadyResolved
        );
        require!(
            market.start_price.is_none(),
            OutcomeMarketsError::StartPriceAlreadySet
        );
        require!(
            clock.unix_timestamp >= market.start_time,
            OutcomeMarketsError::MarketStartTimeNotReached
        );

        let price = read_verified_price(&ctx.accounts.price_update, &market.price_feed_id)?;
        require_boundary_crossing_update(&ctx.accounts.price_update, market.start_time)?;

        let start_price = RecordedPrice::from_pyth(price);
        market.start_price = Some(start_price);

        emit!(MarketStartPriceSetEvent {
            market: market_key,
            updater: ctx.accounts.updater.key(),
            price_feed_id: market.price_feed_id,
            start_time: market.start_time,
            start_price,
            emitted_at: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn resolve(ctx: Context<Resolve>) -> Result<()> {
        let market_key = ctx.accounts.market.key();
        let market = &mut ctx.accounts.market;
        let clock = Clock::get()?;

        require!(
            !market.is_resolved(),
            OutcomeMarketsError::MarketAlreadyResolved
        );
        require!(
            clock.unix_timestamp >= market.end_time,
            OutcomeMarketsError::MarketEndTimeNotReached
        );

        let price = read_verified_price(&ctx.accounts.price_update, &market.price_feed_id)?;
        require_boundary_crossing_update(&ctx.accounts.price_update, market.end_time)?;

        let resolved_price = RecordedPrice::from_pyth(price);
        let outcome = market
            .market_type
            .resolve_outcome(market.start_price.as_ref(), &resolved_price)?;

        market.resolution = outcome;
        market.resolved_price = Some(resolved_price);

        emit!(MarketResolvedEvent {
            market: market_key,
            resolver: ctx.accounts.resolver.key(),
            price_feed_id: market.price_feed_id,
            market_type: market.market_type.clone(),
            start_price: market.start_price,
            resolved_price,
            resolution: market.resolution,
            end_time: market.end_time,
            emitted_at: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn claim(ctx: Context<Claim>, amount: u64) -> Result<()> {
        require!(amount > 0, OutcomeMarketsError::InvalidAmount);

        let market = ctx.accounts.market.as_ref();
        let expected_outcome_mint = match market.resolution {
            Resolution::Yes => market.yes_mint,
            Resolution::No => market.no_mint,
            Resolution::Unresolved => {
                return err!(OutcomeMarketsError::MarketNotResolved);
            }
        };
        require_keys_eq!(
            ctx.accounts.outcome_mint.key(),
            expected_outcome_mint,
            OutcomeMarketsError::InvalidOutcomeMint
        );

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.outcome_mint.to_account_info(),
                    from: ctx.accounts.user_outcome_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        transfer_market_collateral(
            market,
            &ctx.accounts.token_program,
            ctx.accounts.collateral_vault.as_ref(),
            ctx.accounts.collateral_mint.as_ref(),
            ctx.accounts.user_collateral_account.as_ref(),
            amount,
        )?;

        emit!(MarketClaimedEvent {
            market: ctx.accounts.market.key(),
            user: ctx.accounts.user.key(),
            collateral_mint: ctx.accounts.collateral_mint.key(),
            collateral_vault: ctx.accounts.collateral_vault.key(),
            outcome_mint: ctx.accounts.outcome_mint.key(),
            user_collateral_account: ctx.accounts.user_collateral_account.key(),
            user_outcome_token_account: ctx.accounts.user_outcome_token_account.key(),
            resolution: market.resolution,
            amount,
            emitted_at: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn initialize_metadata(
        ctx: Context<InitializeMetadata>,
        side: OutcomeSide,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        require!(
            name.len() <= METADATA_NAME_MAX_LEN,
            OutcomeMarketsError::MetadataNameTooLong
        );
        require!(
            symbol.len() <= METADATA_SYMBOL_MAX_LEN,
            OutcomeMarketsError::MetadataSymbolTooLong
        );
        require!(
            uri.len() <= METADATA_URI_MAX_LEN,
            OutcomeMarketsError::MetadataUriTooLong
        );

        let market = ctx.accounts.market.as_ref();
        let expected_mint = match side {
            OutcomeSide::Yes => market.yes_mint,
            OutcomeSide::No => market.no_mint,
        };
        require_keys_eq!(
            ctx.accounts.mint.key(),
            expected_mint,
            OutcomeMarketsError::InvalidOutcomeSide
        );

        let end_time_bytes = market.end_time.to_le_bytes();
        let start_time_bytes = market.start_time.to_le_bytes();
        let signer_seeds: &[&[u8]] = &[
            MARKET_SEED,
            market.price_feed_id.as_ref(),
            end_time_bytes.as_ref(),
            market.market_type_seed.as_ref(),
            start_time_bytes.as_ref(),
            &[market.bump],
        ];

        create_metadata_accounts_v3(
            CpiContext::new_with_signer(
                ctx.accounts.token_metadata_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.metadata.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    mint_authority: ctx.accounts.market.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    update_authority: ctx.accounts.market.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
                &[signer_seeds],
            ),
            DataV2 {
                name: name.clone(),
                symbol: symbol.clone(),
                uri: uri.clone(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            false,
            true,
            None,
        )?;

        emit!(MarketMetadataInitializedEvent {
            market: ctx.accounts.market.key(),
            mint: ctx.accounts.mint.key(),
            metadata: ctx.accounts.metadata.key(),
            side,
            name,
            symbol,
            uri,
            emitted_at: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(params: InitializeMarketParams)]
pub struct InitializeMarket<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = OutcomeMarket::LEN,
        seeds = [
            MARKET_SEED,
            params.price_feed_id.as_ref(),
            &params.end_time.to_le_bytes(),
            params.market_type.seed_bytes().as_ref(),
            &params.start_time.to_le_bytes(),
        ],
        bump
    )]
    pub market: Account<'info, OutcomeMarket>,
    #[account(
        init,
        payer = payer,
        seeds = [market.key().as_ref(), YES_MINT_SEED],
        bump,
        mint::decimals = collateral_mint.decimals,
        mint::authority = market
    )]
    pub yes_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = payer,
        seeds = [market.key().as_ref(), NO_MINT_SEED],
        bump,
        mint::decimals = collateral_mint.decimals,
        mint::authority = market
    )]
    pub no_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = payer,
        seeds = [market.key().as_ref(), COLLATERAL_VAULT_SEED],
        bump,
        token::mint = collateral_mint,
        token::authority = market
    )]
    pub collateral_vault: Account<'info, TokenAccount>,
    pub collateral_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Split<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        has_one = collateral_mint @ OutcomeMarketsError::InvalidCollateralMint,
        has_one = yes_mint @ OutcomeMarketsError::InvalidYesMint,
        has_one = no_mint @ OutcomeMarketsError::InvalidNoMint,
        has_one = collateral_vault @ OutcomeMarketsError::InvalidCollateralVault,
    )]
    pub market: Box<Account<'info, OutcomeMarket>>,
    pub collateral_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub yes_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub no_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = user_collateral_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_collateral_account.mint == collateral_mint.key() @ OutcomeMarketsError::InvalidCollateralAccount,
    )]
    pub user_collateral_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = user_yes_token_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_yes_token_account.mint == yes_mint.key() @ OutcomeMarketsError::InvalidYesTokenAccount,
    )]
    pub user_yes_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = user_no_token_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_no_token_account.mint == no_mint.key() @ OutcomeMarketsError::InvalidNoTokenAccount,
    )]
    pub user_no_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Merge<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        has_one = collateral_mint @ OutcomeMarketsError::InvalidCollateralMint,
        has_one = yes_mint @ OutcomeMarketsError::InvalidYesMint,
        has_one = no_mint @ OutcomeMarketsError::InvalidNoMint,
        has_one = collateral_vault @ OutcomeMarketsError::InvalidCollateralVault,
    )]
    pub market: Box<Account<'info, OutcomeMarket>>,
    pub collateral_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub yes_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub no_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = user_collateral_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_collateral_account.mint == collateral_mint.key() @ OutcomeMarketsError::InvalidCollateralAccount,
    )]
    pub user_collateral_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = user_yes_token_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_yes_token_account.mint == yes_mint.key() @ OutcomeMarketsError::InvalidYesTokenAccount,
    )]
    pub user_yes_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = user_no_token_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_no_token_account.mint == no_mint.key() @ OutcomeMarketsError::InvalidNoTokenAccount,
    )]
    pub user_no_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SetStartPrice<'info> {
    pub updater: Signer<'info>,
    #[account(mut)]
    pub market: Account<'info, OutcomeMarket>,
    pub price_update: Account<'info, PriceUpdateV2>,
}

#[derive(Accounts)]
pub struct Resolve<'info> {
    pub resolver: Signer<'info>,
    #[account(mut)]
    pub market: Account<'info, OutcomeMarket>,
    pub price_update: Account<'info, PriceUpdateV2>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        has_one = collateral_mint @ OutcomeMarketsError::InvalidCollateralMint,
        has_one = collateral_vault @ OutcomeMarketsError::InvalidCollateralVault,
    )]
    pub market: Box<Account<'info, OutcomeMarket>>,
    pub collateral_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub outcome_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        constraint = user_collateral_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_collateral_account.mint == collateral_mint.key() @ OutcomeMarketsError::InvalidCollateralAccount,
    )]
    pub user_collateral_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = user_outcome_token_account.owner == user.key() @ OutcomeMarketsError::InvalidTokenOwner,
        constraint = user_outcome_token_account.mint == outcome_mint.key() @ OutcomeMarketsError::InvalidOutcomeTokenAccount,
    )]
    pub user_outcome_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct InitializeMetadata<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub market: Box<Account<'info, OutcomeMarket>>,
    /// CHECK: Constrained against `market.yes_mint` / `market.no_mint` in the handler.
    /// Mutability is required because Metaplex updates the mint account during
    /// `CreateMetadataAccountV3` (it sanity-checks the mint authority).
    #[account(mut)]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: Address is enforced by the Token Metadata program; the CPI fails if the
    /// PDA does not match `["metadata", token_metadata_program, mint]`.
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,
    pub token_metadata_program: Program<'info, Metadata>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

fn read_verified_price(price_update: &PriceUpdateV2, feed_id: &[u8; 32]) -> Result<Price> {
    require!(
        price_update.verification_level.gte(VerificationLevel::Full),
        OutcomeMarketsError::InsufficientPythVerification
    );

    price_update
        .get_price_unchecked(feed_id)
        .map_err(|_| error!(OutcomeMarketsError::InvalidPriceUpdate))
}

fn require_boundary_crossing_update(
    price_update: &PriceUpdateV2,
    boundary_time: i64,
) -> Result<()> {
    require!(
        price_update.price_message.prev_publish_time < boundary_time
            && boundary_time <= price_update.price_message.publish_time,
        OutcomeMarketsError::OracleUpdateDoesNotCrossBoundary
    );

    Ok(())
}

fn transfer_market_collateral<'info>(
    market: &Account<'info, OutcomeMarket>,
    token_program: &Program<'info, Token>,
    collateral_vault: &Account<'info, TokenAccount>,
    collateral_mint: &Account<'info, Mint>,
    destination: &Account<'info, TokenAccount>,
    amount: u64,
) -> Result<()> {
    let end_time_bytes = market.end_time.to_le_bytes();
    let start_time_bytes = market.start_time.to_le_bytes();
    let signer_seeds = &[
        MARKET_SEED,
        market.price_feed_id.as_ref(),
        end_time_bytes.as_ref(),
        market.market_type_seed.as_ref(),
        start_time_bytes.as_ref(),
        &[market.bump],
    ];

    token::transfer_checked(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            TransferChecked {
                from: collateral_vault.to_account_info(),
                mint: collateral_mint.to_account_info(),
                to: destination.to_account_info(),
                authority: market.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
        collateral_mint.decimals,
    )
}
