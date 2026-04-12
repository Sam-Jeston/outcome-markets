use std::{fs, process::Command, str::FromStr};

use solana_account::{Account, WritableAccount};
use solana_pubkey::Pubkey;

pub fn load_account_fixture(key: &str) -> Account {
    let path = format!("tests/fixtures/{key}.json");
    let mut fs_result = fs::read_to_string(&path);

    if fs_result.is_err() {
        let output = Command::new("solana")
            .arg("account")
            .arg(key)
            .arg("--output")
            .arg("json")
            .output()
            .unwrap();

        if !output.stderr.is_empty() {
            println!("Failed to read account from solana: {:#?}", output.stderr);
            panic!("Failed to read account {key}");
        }

        fs::write(&path, output.stdout).unwrap();
        fs_result = fs::read_to_string(&path);
    }

    let data: serde_json::Value = serde_json::from_str(&fs_result.unwrap()).unwrap();
    let lamports = data["account"]["lamports"].as_u64().unwrap();
    let owner = Pubkey::from_str(data["account"]["owner"].as_str().unwrap()).unwrap();
    let account_data = base64::decode(data["account"]["data"][0].as_str().unwrap()).unwrap();
    let rent_epoch = data["account"]["rentEpoch"].as_u64().unwrap();

    WritableAccount::create(
        lamports,
        account_data,
        owner,
        data["account"]["executable"].as_bool().unwrap(),
        rent_epoch,
    )
}
