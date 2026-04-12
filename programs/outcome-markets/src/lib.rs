use anchor_lang::prelude::*;

declare_id!("23uBqw2FZEUAj5JtTuzCHidyijuNZQmqvMTDPAjXJp6U");

#[program]
pub mod outcome_markets {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
