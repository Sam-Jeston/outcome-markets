use std::path::PathBuf;

use litesvm::LiteSVM;
use solana_pubkey::Pubkey;

pub fn load_outcome_markets_program(svm: &mut LiteSVM) -> Pubkey {
    let program_path = std::env::var("OUTCOME_MARKETS_SO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/outcome_markets.so")
        });

    let program_bytes = std::fs::read(&program_path).unwrap_or_else(|err| {
        panic!(
            "Failed to read outcome_markets.so at '{}': {}. Build the program with `anchor build` or set OUTCOME_MARKETS_SO.",
            program_path.display(),
            err
        )
    });

    let program_id = outcome_markets::id().to_bytes().into();
    svm.add_program(program_id, &program_bytes).unwrap();
    program_id
}
