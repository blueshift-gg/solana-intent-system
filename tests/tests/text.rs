use mandate_core::terms::{Decay, Per, Price, Terms};
use mandate_tests::*;
use solana_address::Address;

const AUTHORITY: &str = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
const SPENDER: &str = "GNxM82DJMja5ux5extFCEjbQ5C88hvcG7fvsiSCQumgs";
const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const FROM: &str = "4dEfGh1uC6pK4CwNa5oZ2bJwmWv6kD6YQX7sKfM5tR2b";
const TO: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
const MONTHLY: Per = Per::Every(2_592_000);

fn key(s: &str) -> [u8; 32] {
    Address::from_str_const(s).to_bytes()
}

fn decimals(mint: &[u8; 32]) -> Result<u8, mandate_core::errors::MandateError> {
    Ok(if *mint == key(USDC_MINT) { 6 } else { 9 })
}

fn decoded(terms: &Terms) -> bool {
    Terms::decode(&encode(terms)).is_ok()
}

/// The canonical text is what wallets show and authorities sign; this pins it
/// byte for byte.
#[test]
fn subscription_renders_its_canonical_text() {
    let (authority, spender, usdc, from) =
        (key(AUTHORITY), key(SPENDER), key(USDC_MINT), key(FROM));
    let limits = [limit(&from, &usdc, 8 * USDC, MONTHLY)];
    let terms = terms(&authority, Some(&spender), None, &limits, None);

    assert_eq!(
        text(&terms, decimals),
        "Solana Mandate v1
cluster: localnet
engine: Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A
authority: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
SPENDER: GNxM82DJMja5ux5extFCEjbQ5C88hvcG7fvsiSCQumgs
MAY TAKE: at most 8.000000 of mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v from 4dEfGh1uC6pK4CwNa5oZ2bJwmWv6kD6YQX7sKfM5tR2b every 30d
VALID: from 2026-09-21T14:13:20Z until revoked
SALT: 0"
    );
}

#[test]
fn order_renders_its_canonical_text() {
    let (authority, usdc, sol) = (key(AUTHORITY), key(USDC_MINT), key(SOL_MINT));
    let (from, to) = (key(FROM), key(TO));
    let limits = [limit(&from, &usdc, 100 * USDC, Per::Total)];
    let price = Price {
        to: &to,
        mint: &sol,
        num: 5_200_000,
        den: USDC,
        decay: Some(Decay {
            t0: NOW,
            t1: NOW + 300,
            num: 5_000_000,
        }),
    };
    let terms = terms(&authority, None, Some(NOW + 600), &limits, Some(price));

    assert_eq!(
        text(&terms, decimals),
        "Solana Mandate v1
cluster: localnet
engine: Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A
authority: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
SPENDER: anyone
MAY TAKE: at most 100.000000 of mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v from 4dEfGh1uC6pK4CwNa5oZ2bJwmWv6kD6YQX7sKfM5tR2b in total
PRICE: at least 0.005200000 of mint So11111111111111111111111111111111111111112 to 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM for every 1.000000 taken, moving to 0.005000000 from 2026-09-21T14:13:20Z to 2026-09-21T14:18:20Z
VALID: from 2026-09-21T14:13:20Z until 2026-09-21T14:23:20Z
SALT: 0"
    );
}

#[test]
fn terms_round_trip_and_invalid_terms_are_refused() {
    let (authority, spender, usdc, sol) =
        (key(AUTHORITY), key(SPENDER), key(USDC_MINT), key(SOL_MINT));
    let (from, to) = (key(FROM), key(TO));
    let limits = [
        limit(&from, &usdc, 10 * USDC, MONTHLY),
        limit(&from, &usdc, USDC, Per::Use),
    ];
    let price = Price {
        to: &to,
        mint: &sol,
        num: 3,
        den: 2,
        decay: None,
    };
    let valid = terms(&authority, Some(&spender), None, &limits, Some(price));
    let bytes = encode(&valid);
    let back = Terms::decode(&bytes).unwrap();
    assert_eq!(encode(&back), bytes);
    assert_eq!(back.limits(), limits);
    assert_eq!(back.price, Some(price));

    // Nobody bound: anyone may spend, and the owner is paid nothing
    assert!(!decoded(&terms(&authority, None, None, &limits, None)));
    assert!(decoded(&terms(
        &authority,
        None,
        None,
        &limits,
        Some(price)
    )));
    // A per-use cap with no limit that persists
    let alone = [limit(&from, &usdc, USDC, Per::Use)];
    assert!(!decoded(&terms(
        &authority,
        Some(&spender),
        None,
        &alone,
        None
    )));
    // A price over two different sources
    let two = [
        limit(&from, &usdc, USDC, Per::Total),
        limit(&to, &sol, SOL, Per::Total),
    ];
    assert!(decoded(&terms(
        &authority,
        Some(&spender),
        None,
        &two,
        None
    )));
    assert!(!decoded(&terms(
        &authority,
        Some(&spender),
        None,
        &two,
        Some(price)
    )));
    // No limits, and more than the maximum
    assert!(!decoded(&terms(
        &authority,
        Some(&spender),
        None,
        &[],
        None
    )));
    let many = [limit(&from, &usdc, USDC, Per::Total); 9];
    assert!(decoded(&terms(
        &authority,
        Some(&spender),
        None,
        &many[..8],
        None
    )));
    assert!(!decoded(&terms(
        &authority,
        Some(&spender),
        None,
        &many,
        None
    )));
}

/// What a wallet shows must pin the terms exactly: changing any byte of the
/// encoding either fails to decode or changes the text. One signature can
/// then never authorize two different policies.
#[test]
fn every_byte_of_the_terms_is_visible_in_the_text() {
    let (authority, spender, usdc, sol) =
        (key(AUTHORITY), key(SPENDER), key(USDC_MINT), key(SOL_MINT));
    let (from, to) = (key(FROM), key(TO));
    let limits = [
        limit(&from, &usdc, 10 * USDC, MONTHLY),
        limit(&from, &usdc, USDC, Per::Use),
        limit(&from, &usdc, 100 * USDC, Per::Total),
    ];
    let price = Price {
        to: &to,
        mint: &sol,
        num: 5_200_000,
        den: USDC,
        decay: Some(Decay {
            t0: NOW,
            t1: NOW + 300,
            num: 5_000_000,
        }),
    };
    let mut order = terms(&authority, None, Some(NOW + 600), &limits[2..], Some(price));
    order.salt = 7;
    let subscription = terms(&authority, Some(&spender), None, &limits, None);

    let render = |bytes: &[u8]| {
        let terms = Terms::decode(bytes).ok()?;
        let mut out = Vec::new();
        mandate_core::render::render(&terms, |_| Ok(6), &mut out).ok()?;
        Some(out)
    };
    for bytes in [encode(&order), encode(&subscription)] {
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
