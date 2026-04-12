use litesvm::LiteSVM;
use solana_account::Account;
use solana_sdk::{program_option::COption, program_pack::Pack, pubkey::Pubkey};
use spl_token::state::{Account as TokenAccount, AccountState, Mint as TokenMint};

pub fn create_mint_account(svm: &mut LiteSVM, decimals: u8) -> Pubkey {
    let mint_key = Pubkey::new_unique();
    let mint_account = TokenMint {
        mint_authority: COption::Some(Pubkey::new_unique()),
        supply: 0,
        decimals,
        is_initialized: true,
        freeze_authority: COption::None,
    };

    let mut mint_account_bytes = [0u8; TokenMint::LEN];
    TokenMint::pack(mint_account, &mut mint_account_bytes).unwrap();

    svm.set_account(
        mint_key.to_bytes().into(),
        Account {
            lamports: 1_000_000_000,
            data: mint_account_bytes.to_vec(),
            owner: spl_token::ID.to_bytes().into(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    mint_key
}

pub fn create_token_account(
    svm: &mut LiteSVM,
    owner: &Pubkey,
    mint: &Pubkey,
    amount: u64,
) -> Pubkey {
    let token_account_key = Pubkey::new_unique();
    let token_account = TokenAccount {
        mint: *mint,
        owner: *owner,
        amount,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut token_account_bytes = [0u8; TokenAccount::LEN];
    TokenAccount::pack(token_account, &mut token_account_bytes).unwrap();

    svm.set_account(
        token_account_key.to_bytes().into(),
        Account {
            lamports: 1_000_000_000,
            data: token_account_bytes.to_vec(),
            owner: spl_token::ID.to_bytes().into(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    token_account_key
}
