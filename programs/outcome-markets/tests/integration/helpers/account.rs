use litesvm::LiteSVM;
use solana_account::Account;
use solana_sdk::pubkey::Pubkey;

use crate::helpers::load_fixture::load_account_fixture;

pub fn load_account(svm: &mut LiteSVM, key: &Pubkey) -> Account {
    let account = load_account_fixture(key.to_string().as_str());
    svm.set_account(key.to_bytes().into(), account.clone())
        .unwrap();
    account
}
