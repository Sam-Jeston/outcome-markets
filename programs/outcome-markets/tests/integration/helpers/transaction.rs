use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{v0, AddressLookupTableAccount, Instruction, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_transaction::versioned::VersionedTransaction;

pub fn prepare_v0_tx(
    svm: &mut LiteSVM,
    payer: &Pubkey,
    signers: &[&Keypair],
    address_lookup_table_accounts: &[AddressLookupTableAccount],
    instructions: &[Instruction],
) -> VersionedTransaction {
    let blockhash = svm.latest_blockhash();
    let message = v0::Message::try_compile(
        payer,
        instructions,
        address_lookup_table_accounts,
        blockhash,
    )
    .unwrap();

    VersionedTransaction::try_new(VersionedMessage::V0(message), signers).unwrap()
}
