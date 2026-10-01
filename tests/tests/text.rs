use mandate_core::terms::{Bound, Refill, Terms};
use mandate_tests::*;
use solana_address::Address;

const AUTHORITY: &str = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const FROM: &str = "4dEfGh1uC6pK4CwNa5oZ2bJwmWv6kD6YQX7sKfM5tR2b";
const TO: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
const DECAY: Bound = Bound::Linear {
    t0: NOW,
    v0: 520_000_000,
    t1: NOW + 300,
    v1: 500_000_000,
};
const MONTHLY: Refill = Refill::Over { period: 2_592_000 };

fn key(s: &str) -> [u8; 32] {
    Address::from_str_const(s).to_bytes()
}

fn decimals(mint: &[u8; 32]) -> Result<u8, mandate_core::errors::MandateError> {
    Ok(if *mint == key(USDC_MINT) { 6 } else { 9 })
}

/// The canonical text is what wallets show and authorities sign; this pins it
/// byte for byte.
#[test]
fn swap_intent_renders_its_canonical_text() {
    let (authority, usdc, sol) = (key(AUTHORITY), key(USDC_MINT), key(SOL_MINT));
    let (from, to) = (key(FROM), key(TO));
    let takes = [take(&from, &usdc, 100 * USDC, Refill::Never)];
    let requires = [gain(&to, &sol, &authority, DECAY)];
    let terms = terms(&authority, true, Some(NOW + 300), &takes, &requires);

    assert_eq!(
        text(&terms, decimals),
        "Solana Mandate v1
cluster: localnet
engine: Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A
authority: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
[0] MAY TAKE: at most 100.000000 of mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v from 4dEfGh1uC6pK4CwNa5oZ2bJwmWv6kD6YQX7sKfM5tR2b in total
[1] REQUIRES: 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM (mint So11111111111111111111111111111111111111112, owner 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU) gains at least 0.520000000 at 2026-09-21T14:13:20Z moving linearly to 0.500000000 at 2026-09-21T14:18:20Z
EXECUTOR: anyone
VALID: from 2026-09-21T14:13:20Z until 2026-09-21T14:18:20Z
REPLAY: once
EPOCH: 0"
    );
}

#[test]
fn subscription_renders_its_canonical_text() {
    let (authority, usdc, from, to) = (key(AUTHORITY), key(USDC_MINT), key(FROM), [key(TO)]);
    let takes = [pay(&from, &usdc, 8 * USDC, MONTHLY, &to)];
    let terms = terms(&authority, false, Some(NOW + 365 * 86_400), &takes, &[]);

    assert_eq!(
        text(&terms, decimals),
        "Solana Mandate v1
cluster: localnet
engine: Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A
authority: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
[0] MAY PAY: at most 8.000000 of mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v from 4dEfGh1uC6pK4CwNa5oZ2bJwmWv6kD6YQX7sKfM5tR2b to 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM, refilling over 30d
EXECUTOR: anyone
VALID: from 2026-09-21T14:13:20Z until 2027-09-21T14:13:20Z
REPLAY: any number of times
EPOCH: 0"
    );
}

#[test]
fn terms_round_trip_through_the_canonical_encoding() {
    let (authority, usdc, sol) = (key(AUTHORITY), key(USDC_MINT), key(SOL_MINT));
    let (from, to) = (key(FROM), [key(TO), key(SOL_MINT)]);
    let takes = [
        pay(&from, &usdc, 10 * USDC, MONTHLY, &to),
        take(&from, &usdc, USDC, Refill::EachUse),
    ];
    let bytes = encode(&terms(&authority, false, None, &takes, &[]));
    let decoded = Terms::decode(&bytes).unwrap();
    assert_eq!(encode(&decoded), bytes);
    assert_eq!(decoded.takes.iter().collect::<Vec<_>>(), takes);

    // A per-use cap with no limit that persists is refused, unless the mandate runs once
    let alone = [take(&from, &usdc, USDC, Refill::EachUse)];
    assert!(Terms::decode(&encode(&terms(&authority, false, None, &alone, &[]))).is_err());
    assert!(Terms::decode(&encode(&terms(&authority, true, None, &alone, &[]))).is_ok());

    let ratio = Bound::Ratio {
        of: 0,
        num: 1,
        den: 1,
    };
    let takes = [take(&from, &usdc, 10 * USDC, Refill::Never)];
    let requires = [gain(&to[0], &sol, &authority, ratio)];
    let bytes = encode(&terms(&authority, false, None, &takes, &requires));
    let decoded = Terms::decode(&bytes).unwrap();
    assert_eq!(encode(&decoded), bytes);
    assert_eq!(decoded.requires.iter().collect::<Vec<_>>(), requires);
}

/// What a wallet shows must pin the terms exactly: changing any byte of the
/// encoding either fails to decode or changes the text. One signature can
/// then never authorize two different mandates.
#[test]
fn every_byte_of_the_terms_is_visible_in_the_text() {
    let (authority, usdc, sol) = (key(AUTHORITY), key(USDC_MINT), key(SOL_MINT));
    let (from, to) = (key(FROM), [key(TO), key(SOL_MINT)]);
    let swap_takes = [take(&from, &usdc, 100 * USDC, Refill::Never)];
    let swap_requires = [gain(&to[0], &sol, &authority, DECAY)];
    let swap = terms(
        &authority,
        true,
        Some(NOW + 300),
        &swap_takes,
        &swap_requires,
    );
    let payment_takes = [
        pay(&from, &usdc, 10 * USDC, MONTHLY, &to),
        take(&from, &usdc, USDC, Refill::EachUse),
    ];
    let payment = terms(&authority, false, None, &payment_takes, &[]);

    let render = |bytes: &[u8]| {
        let terms = Terms::decode(bytes).ok()?;
        let mut out = Vec::new();
        mandate_core::render::render(&terms, |_| Ok(6), &mut out).ok()?;
        Some(out)
    };
    for bytes in [encode(&swap), encode(&payment)] {
        let original = render(&bytes).unwrap();
        for i in 0..bytes.len() {
            for v in 0..=u8::MAX {
                let mut mutated = bytes.clone();
                if mutated[i] == v {
                    continue;
                }
                mutated[i] = v;
                if let Some(text) = render(&mutated) {
                    assert_ne!(text, original, "byte {i} = {v} is invisible in the text");
                }
            }
        }
    }
}
