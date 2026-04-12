use anchor_lang::AccountSerialize;
use litesvm::LiteSVM;
use pyth_solana_receiver_sdk::price_update::{PriceUpdateV2, VerificationLevel};
use pythnet_sdk::messages::PriceFeedMessage;
use solana_account::Account;
use solana_sdk::pubkey::Pubkey;

pub fn create_price_update_account(
    svm: &mut LiteSVM,
    account_key: &Pubkey,
    feed_id: [u8; 32],
    price: i64,
    exponent: i32,
    publish_time: i64,
) {
    let price_update = PriceUpdateV2 {
        write_authority: Pubkey::new_unique(),
        verification_level: VerificationLevel::Full,
        price_message: PriceFeedMessage {
            feed_id,
            price,
            conf: 0,
            exponent,
            publish_time,
            prev_publish_time: publish_time.saturating_sub(1),
            ema_price: price,
            ema_conf: 0,
        },
        posted_slot: 0,
    };

    let mut data = Vec::new();
    price_update.try_serialize(&mut data).unwrap();

    svm.set_account(
        account_key.to_bytes().into(),
        Account {
            lamports: 1_000_000_000,
            data,
            owner: pyth_solana_receiver_sdk::ID.to_bytes().into(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}
