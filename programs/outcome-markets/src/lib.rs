use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, TransferChecked};
use pyth_solana_receiver_sdk::price_update::{Price, PriceUpdateV2, VerificationLevel};

pub mod constants;
pub mod error;
pub mod state;

use constants::{COLLATERAL_VAULT_SEED, MARKET_SEED, NO_MINT_SEED, USDC_DECIMALS, YES_MINT_SEED};
use error::OutcomeMarketsError;
use state::{InitializeMarketParams, OutcomeMarket, RecordedPrice, Resolution};

declare_id!("23uBqw2FZEUAj5JtTuzCHidyijuNZQmqvMTDPAjXJp6U");

#[program]
pub mod outcome_markets {
    use super::*;

    pub fn initialize_market(
        ctx: Context<InitializeMarket>,
        params: InitializeMarketParams,
    ) -> Result<()> {
        params.validate()?;

        require!(
            Clock::get()?.unix_timestamp < params.end_time,
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

        Ok(())
    }

    pub fn split(ctx: Context<Split>, amount: u64) -> Result<()> {
        require!(amount > 0, OutcomeMarketsError::InvalidAmount);
        require!(
            !ctx.accounts.market.is_resolved(),
            OutcomeMarketsError::MarketAlreadyResolved
        );
        require!(
            Clock::get()?.unix_timestamp < ctx.accounts.market.end_time,
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
        )
    }

    pub fn set_start_price(ctx: Context<SetStartPrice>) -> Result<()> {
        let market = &mut ctx.accounts.market;

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

        let price = read_verified_price(&ctx.accounts.price_update, &market.price_feed_id)?;
        require!(
            price.publish_time >= market.start_time,
            OutcomeMarketsError::OracleUpdateTooEarly
        );

        market.start_price = Some(RecordedPrice::from_pyth(price));

        Ok(())
    }

    pub fn resolve(ctx: Context<Resolve>) -> Result<()> {
        let market = &mut ctx.accounts.market;

        require!(
            !market.is_resolved(),
            OutcomeMarketsError::MarketAlreadyResolved
        );

        let price = read_verified_price(&ctx.accounts.price_update, &market.price_feed_id)?;
        require!(
            price.publish_time >= market.end_time,
            OutcomeMarketsError::OracleUpdateTooEarly
        );

        let resolved_price = RecordedPrice::from_pyth(price);
        let outcome = market
            .market_type
            .resolve_outcome(market.start_price.as_ref(), &resolved_price)?;

        market.resolution = outcome;
        market.resolved_price = Some(resolved_price);

        Ok(())
    }

    pub fn claim(ctx: Context<Claim>, amount: u64) -> Result<()> {
        require!(amount > 0, OutcomeMarketsError::InvalidAmount);

        let market = ctx.accounts.market.as_ref();
        let (winning_mint, winning_token_account) = match market.resolution {
            Resolution::Yes => (
                ctx.accounts.yes_mint.as_ref(),
                ctx.accounts.user_yes_token_account.as_ref(),
            ),
            Resolution::No => (
                ctx.accounts.no_mint.as_ref(),
                ctx.accounts.user_no_token_account.as_ref(),
            ),
            Resolution::Unresolved => {
                return err!(OutcomeMarketsError::MarketNotResolved);
            }
        };

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: winning_mint.to_account_info(),
                    from: winning_token_account.to_account_info(),
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
        )
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
    #[account(
        constraint = collateral_mint.decimals == USDC_DECIMALS @ OutcomeMarketsError::InvalidCollateralMintDecimals
    )]
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

fn read_verified_price(price_update: &PriceUpdateV2, feed_id: &[u8; 32]) -> Result<Price> {
    require!(
        price_update.verification_level.gte(VerificationLevel::Full),
        OutcomeMarketsError::InsufficientPythVerification
    );

    price_update
        .get_price_unchecked(feed_id)
        .map_err(|_| error!(OutcomeMarketsError::InvalidPriceUpdate))
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
