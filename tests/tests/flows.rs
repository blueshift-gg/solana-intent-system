#![allow(clippy::result_large_err)] // LiteSVM's own result type

use mandate_core::constants::*;
use mandate_core::errors::MandateError;
use mandate_core::terms::{Decay, Per, Price};
use mandate_tests::*;
use solana_address::Address;
use solana_signer::Signer;

const DAY: i64 = 86_400;
const MONTHLY: Per = Per::Every(30 * DAY as u32);

/// Whether `result` failed with exactly this program error.
fn refused(result: litesvm::types::TransactionResult, error: MandateError) -> bool {
    let wanted = format!("Custom({})", error as u32);
    result.is_err_and(|e| format!("{:?}", e.err).contains(&wanted))
}

#[test]
fn subscription_is_one_instruction_and_resets_each_period() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let (merchant, stranger) = (f.wallet(0, 0), f.wallet(0, 0));
    let (u, m, usdc) = (
        user.address().to_bytes(),
        merchant.address().to_bytes(),
        f.usdc.to_bytes(),
    );
    let user_usdc = user.usdc.to_bytes();

    // The merchant may take at most 10 USDC every 30 days, until revoked
    let limits = [limit(&user_usdc, &usdc, 10 * USDC, MONTHLY)];
    let bytes = encode(&terms(&u, Some(&m), None, &limits, None));
    let create = create(&user.address(), &user.address(), &bytes, None);
    f.send(&[create], &[&user.key]).unwrap();

    let pull = |f: &mut Fixture, day: i64, who: &Wallet, amount: u64| {
        f.set_time(NOW + day * DAY);
        let accounts = (user.usdc, f.usdc, who.usdc);
        let ix = pull(
            &who.address(),
            &user.address(),
            &bytes,
            accounts,
            amount,
            None,
            TOKEN,
        );
        f.send(&[ix], &[&who.key])
    };
    assert!(refused(
        pull(&mut f, 0, &stranger, USDC),
        MandateError::InvalidSpender
    ));
    pull(&mut f, 0, &merchant, 10 * USDC).unwrap();
    // Nothing more this period; a new period starts full, and unused amounts do not carry over
    assert!(refused(
        pull(&mut f, 29, &merchant, 1),
        MandateError::LimitExceeded
    ));
    pull(&mut f, 30, &merchant, 4 * USDC).unwrap();
    pull(&mut f, 75, &merchant, 10 * USDC).unwrap();
    assert!(refused(
        pull(&mut f, 89, &merchant, 1),
        MandateError::LimitExceeded
    ));

    assert_eq!(f.balance(&user.usdc), 76 * USDC);
    assert_eq!(f.balance(&merchant.usdc), 24 * USDC);
}

#[test]
fn a_program_collects_a_subscription_by_cpi() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let merchant = f.wallet(0, 0);
    let (u, m, usdc) = (
        user.address().to_bytes(),
        merchant.address().to_bytes(),
        f.usdc.to_bytes(),
    );
    let user_usdc = user.usdc.to_bytes();

    let limits = [limit(&user_usdc, &usdc, 10 * USDC, MONTHLY)];
    let bytes = encode(&terms(&u, Some(&m), None, &limits, None));
    let create = create(&user.address(), &user.address(), &bytes, None);
    f.send(&[create], &[&user.key]).unwrap();

    // The merchant's own program makes the pull, as it would while renewing a membership
    let accounts = (user.usdc, f.usdc, merchant.usdc);
    let m = merchant.address();
    let ix = by_cpi(pull(
        &m,
        &user.address(),
        &bytes,
        accounts,
        10 * USDC,
        None,
        TOKEN,
    ));
    f.send(std::slice::from_ref(&ix), &[&merchant.key]).unwrap();
    assert!(refused(
        f.send(&[ix], &[&merchant.key]),
        MandateError::LimitExceeded
    ));
    assert_eq!(f.balance(&merchant.usdc), 10 * USDC);
}

#[test]
fn a_signature_creates_the_mandate_and_the_spender_pays_the_rent() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let merchant = f.wallet(0, 0);
    let (u, m, usdc) = (
        user.address().to_bytes(),
        merchant.address().to_bytes(),
        f.usdc.to_bytes(),
    );
    let user_usdc = user.usdc.to_bytes();
    let limits = [limit(&user_usdc, &usdc, 10 * USDC, MONTHLY)];
    let (owner, spender) = (user.address(), merchant.address());

    // A signature can only stand behind a mandate that expires
    let forever = terms(&u, Some(&m), None, &limits, None);
    let signature = sign(&forever, &user.key, f.decimals());
    let ix = create(
        &owner,
        &spender,
        &encode(&forever),
        Some((&signature, &[f.usdc])),
    );
    assert!(refused(
        f.send(&[ix], &[&merchant.key]),
        MandateError::ExpiryRequired
    ));

    // Signed once, off chain, for a year. The merchant brings it with its first pull
    let year = terms(&u, Some(&m), Some(NOW + 365 * DAY), &limits, None);
    let signature = sign(&year, &user.key, f.decimals());
    let bytes = encode(&year);
    let accounts = (user.usdc, f.usdc, merchant.usdc);
    let pull = pull(&spender, &owner, &bytes, accounts, 10 * USDC, None, TOKEN);
    let create = create(&owner, &spender, &bytes, Some((&signature, &[f.usdc])));
    f.send(&[create.clone(), pull.clone()], &[&merchant.key])
        .unwrap();
    f.set_time(NOW + 30 * DAY);
    f.send(std::slice::from_ref(&pull), &[&merchant.key])
        .unwrap();
    assert_eq!(f.balance(&merchant.usdc), 20 * USDC);

    // The user cancels. The account stays, revoked, so the signature cannot
    // create the mandate again; after the expiry anyone returns the rent to the merchant
    let close = close(&owner, &owner, &bytes, &spender);
    f.send(std::slice::from_ref(&close), &[&user.key]).unwrap();
    f.set_time(NOW + 60 * DAY);
    assert!(refused(
        f.send(&[pull], &[&merchant.key]),
        MandateError::Revoked
    ));
    assert!(refused(
        f.send(&[create], &[&merchant.key]),
        MandateError::AlreadyInitialized
    ));
    let mandate = mandate_pda(&owner, &bytes);
    let before = f.svm.get_balance(&spender).unwrap();
    f.set_time(NOW + 365 * DAY);
    let stranger = f.payer();
    let reclaim = mandate_tests::close(&stranger.pubkey(), &owner, &bytes, &spender);
    f.send(&[reclaim], &[&stranger]).unwrap();
    assert!(f.svm.get_account(&mandate).is_none_or(|a| a.lamports == 0));
    assert!(f.svm.get_balance(&spender).unwrap() > before);
}

#[test]
fn closing_returns_the_rent_to_whoever_paid_it() {
    let mut f = Fixture::new();
    let user = f.wallet(10 * USDC, 0);
    let (merchant, sponsor, stranger) = (f.wallet(0, 0), f.payer(), f.payer());
    let (u, m, usdc) = (
        user.address().to_bytes(),
        merchant.address().to_bytes(),
        f.usdc.to_bytes(),
    );
    let user_usdc = user.usdc.to_bytes();
    let (owner, payer) = (user.address(), sponsor.pubkey());

    // The user signs the transaction; a sponsor pays the fee and the rent
    let limits = [limit(&user_usdc, &usdc, USDC, MONTHLY)];
    let mut both = Vec::new();
    for salt in [1, 2] {
        let mut terms = terms(&u, Some(&m), None, &limits, None);
        terms.salt = salt;
        let bytes = encode(&terms);
        let create = create(&owner, &payer, &bytes, None);
        f.send(&[create], &[&sponsor, &user.key]).unwrap();
        both.push(bytes);
    }
    let funded = f.svm.get_balance(&payer).unwrap();

    // A stranger cannot close it. The owner can, and so can the spender;
    // a mandate that never expires closes at once and the sponsor is repaid
    let ix = close(&stranger.pubkey(), &owner, &both[0], &payer);
    assert!(refused(
        f.send(&[ix], &[&stranger]),
        MandateError::NotClosable
    ));
    let ix = close(&owner, &owner, &both[0], &owner);
    assert!(refused(
        f.send(&[ix], &[&user.key]),
        MandateError::InvalidPayer
    ));
    let ix = close(&owner, &owner, &both[0], &payer);
    f.send(&[ix], &[&user.key]).unwrap();
    let ix = close(&merchant.address(), &owner, &both[1], &payer);
    f.send(&[ix], &[&merchant.key]).unwrap();

    for bytes in &both {
        let mandate = mandate_pda(&owner, bytes);
        assert!(f.svm.get_account(&mandate).is_none_or(|a| a.lamports == 0));
    }
    assert!(f.svm.get_balance(&payer).unwrap() > funded);
}

#[test]
fn limits_stack_on_one_account() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let agent = f.wallet(0, 0);
    let (u, a, usdc) = (
        user.address().to_bytes(),
        agent.address().to_bytes(),
        f.usdc.to_bytes(),
    );
    let user_usdc = user.usdc.to_bytes();

    // An agent may spend at most 5 USDC per use, 12 a day and 20 in all
    let limits = [
        limit(&user_usdc, &usdc, 5 * USDC, Per::Use),
        limit(&user_usdc, &usdc, 12 * USDC, Per::Every(DAY as u32)),
        limit(&user_usdc, &usdc, 20 * USDC, Per::Total),
    ];
    let bytes = encode(&terms(&u, Some(&a), None, &limits, None));
    let create = create(&user.address(), &user.address(), &bytes, None);
    f.send(&[create], &[&user.key]).unwrap();

    let spend = |f: &mut Fixture, amount: u64| {
        let accounts = (user.usdc, f.usdc, agent.usdc);
        let ix = pull(
            &agent.address(),
            &user.address(),
            &bytes,
            accounts,
            amount,
            None,
            TOKEN,
        );
        f.send(&[ix], &[&agent.key])
    };
    assert!(refused(
        spend(&mut f, 5 * USDC + 1),
        MandateError::LimitExceeded
    ));
    spend(&mut f, 5 * USDC).unwrap();
    spend(&mut f, 5 * USDC).unwrap();
    assert!(refused(
        spend(&mut f, 2 * USDC + 1),
        MandateError::LimitExceeded
    ));
    spend(&mut f, 2 * USDC).unwrap();
    f.set_time(NOW + DAY);
    spend(&mut f, 5 * USDC).unwrap();
    spend(&mut f, 3 * USDC).unwrap();
    assert!(refused(spend(&mut f, 1), MandateError::LimitExceeded));
    assert_eq!(f.balance(&agent.usdc), 20 * USDC);
}

#[test]
fn anyone_fills_a_signed_order_in_parts_at_a_decaying_price() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let (first, second) = (f.wallet(0, 10 * SOL), f.wallet(0, 10 * SOL));
    let (u, usdc, sol) = (
        user.address().to_bytes(),
        f.usdc.to_bytes(),
        f.sol.to_bytes(),
    );
    let (user_usdc, user_sol) = (user.usdc.to_bytes(), user.sol.to_bytes());

    // Sell at most 100 USDC to anyone, for at least 0.0052 SOL each, falling
    // to 0.0050 over five minutes
    let limits = [limit(&user_usdc, &usdc, 100 * USDC, Per::Total)];
    let price = Price {
        to: &user_sol,
        mint: &sol,
        num: 5_200_000,
        den: USDC,
        decay: Some(Decay {
            t0: NOW,
            t1: NOW + 300,
            num: 5_000_000,
        }),
    };
    let terms = terms(&u, None, Some(NOW + 600), &limits, Some(price));
    let signature = sign(&terms, &user.key, f.decimals());
    let bytes = encode(&terms);
    let owner = user.address();

    let fill = |f: &mut Fixture, solver: &Wallet, amount: u64| {
        let take = (user.usdc, f.usdc, solver.usdc);
        let pay = Some((solver.sol, f.sol, user.sol));
        let ix = pull(&solver.address(), &owner, &bytes, take, amount, pay, TOKEN);
        f.send(&[ix], &[&solver.key])
    };

    // Halfway down, one solver brings the signature and takes 30 at 0.0051
    f.set_time(NOW + 150);
    let mints = [f.usdc, f.sol];
    let create = create(&owner, &first.address(), &bytes, Some((&signature, &mints)));
    f.send(&[create], &[&first.key]).unwrap();
    fill(&mut f, &first, 30 * USDC).unwrap();
    assert_eq!(f.balance(&user.sol), 153_000_000);

    // Later another takes the rest at the floor, and the order is spent
    f.set_time(NOW + 400);
    fill(&mut f, &second, 70 * USDC).unwrap();
    assert!(refused(
        fill(&mut f, &second, 1),
        MandateError::LimitExceeded
    ));

    assert_eq!(f.balance(&user.usdc), 0);
    assert_eq!(f.balance(&user.sol), 153_000_000 + 350_000_000);
    assert_eq!(
        (f.balance(&first.usdc), f.balance(&second.usdc)),
        (30 * USDC, 70 * USDC)
    );
}

#[test]
fn a_mandate_reaches_only_its_authoritys_accounts() {
    let mut f = Fixture::new();
    let victim = f.wallet(100 * USDC, 0);
    let thief = f.wallet(0, 0);
    let (t, usdc) = (thief.address().to_bytes(), f.usdc.to_bytes());
    let victim_usdc = victim.usdc.to_bytes();

    // The engine is the victim's delegate too, but this mandate is the thief's
    let limits = [limit(&victim_usdc, &usdc, 100 * USDC, Per::Total)];
    let bytes = encode(&terms(&t, Some(&t), None, &limits, None));
    let key = thief.address();
    f.send(&[create(&key, &key, &bytes, None)], &[&thief.key])
        .unwrap();
    let accounts = (victim.usdc, f.usdc, thief.usdc);
    let ix = pull(&key, &key, &bytes, accounts, USDC, None, TOKEN);
    assert!(refused(
        f.send(&[ix], &[&thief.key]),
        MandateError::InvalidTarget
    ));
    assert_eq!(f.balance(&victim.usdc), 100 * USDC);
}

/// A Token-2022 transfer fee comes out of what arrives. The payer never gives
/// up more than the limit, and a price paid in such a token is refused unless
/// the authority receives all of it.
#[test]
fn a_fee_token_never_shorts_the_authority() {
    use spl_token_2022_interface::extension::transfer_fee::instruction::initialize_transfer_fee_config;
    use spl_token_2022_interface::extension::ExtensionType;
    use spl_token_2022_interface::instruction::{approve, initialize_mint2, mint_to};
    use spl_token_2022_interface::{state::Mint, ID as TOKEN_2022};

    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let merchant = f.wallet(0, 0);
    let issuer = f.payer();

    // A mint that keeps 1% of every transfer
    let mint = Address::new_unique();
    let len = ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferFeeConfig])
        .unwrap();
    let mut account = solana_account::Account::new(SOL, len, &TOKEN_2022);
    account.lamports = f.svm.minimum_balance_for_rent_exemption(len);
    f.svm.set_account(mint, account).unwrap();
    let issuer_key = issuer.pubkey();
    let setup = [
        initialize_transfer_fee_config(&TOKEN_2022, &mint, None, None, 100, u64::MAX).unwrap(),
        initialize_mint2(&TOKEN_2022, &mint, &issuer_key, None, 6).unwrap(),
    ];
    f.send(&setup, &[&issuer]).unwrap();
    let mut account = |owner: &solana_keypair::Keypair| {
        let ata = litesvm_token::CreateAssociatedTokenAccount::new(&mut f.svm, owner, &mint)
            .token_program_id(&TOKEN_2022)
            .send()
            .unwrap();
        let fund = mint_to(&TOKEN_2022, &mint, &ata, &issuer_key, &[], 100 * USDC).unwrap();
        f.send(&[fund], &[&issuer]).unwrap();
        ata
    };
    let (user_fee, merchant_fee) = (account(&user.key), account(&merchant.key));
    let owner = user.address();
    let enable = approve(&TOKEN_2022, &user_fee, &ENGINE_KEY, &owner, &[], u64::MAX).unwrap();
    f.send(&[enable], &[&user.key]).unwrap();
    let held = |f: &Fixture, account: &Address| {
        let data = f.svm.get_account(account).unwrap().data;
        u64::from_le_bytes(data[64..72].try_into().unwrap())
    };
    let (u, m, fee, usdc) = (
        owner.to_bytes(),
        merchant.address().to_bytes(),
        mint.to_bytes(),
        f.usdc.to_bytes(),
    );
    let (user_fee_key, user_usdc) = (user_fee.to_bytes(), user.usdc.to_bytes());
    let spender = merchant.address();

    // Taking the fee token: the user gives up exactly the limit, the merchant gets it less the fee
    let limits = [limit(&user_fee_key, &fee, 10 * USDC, MONTHLY)];
    let bytes = encode(&terms(&u, Some(&m), None, &limits, None));
    f.send(&[create(&owner, &owner, &bytes, None)], &[&user.key])
        .unwrap();
    let accounts = (user_fee, mint, merchant_fee);
    let ix = pull(
        &spender,
        &owner,
        &bytes,
        accounts,
        10 * USDC,
        None,
        TOKEN_2022,
    );
    f.send(&[ix], &[&merchant.key]).unwrap();
    assert_eq!(held(&f, &user_fee), 90 * USDC);
    assert_eq!(held(&f, &merchant_fee), 110 * USDC - USDC / 10);

    // Paying a price in the fee token: what arrives is short, so the pull is refused
    let limits = [limit(&user_usdc, &usdc, 10 * USDC, Per::Total)];
    let price = Price {
        to: &user_fee_key,
        mint: &fee,
        num: 1,
        den: 1,
        decay: None,
    };
    let bytes = encode(&terms(&u, None, None, &limits, Some(price)));
    f.send(&[create(&owner, &owner, &bytes, None)], &[&user.key])
        .unwrap();
    let take = (user.usdc, f.usdc, merchant.usdc);
    let pay = Some((merchant_fee, mint, user_fee));
    let mut ix = pull(&spender, &owner, &bytes, take, 10 * USDC, pay, TOKEN);
    ix.accounts
        .push(solana_instruction::AccountMeta::new_readonly(
            TOKEN_2022, false,
        ));
    assert!(refused(
        f.send(&[ix], &[&merchant.key]),
        MandateError::PriceNotPaid
    ));
    assert_eq!(f.balance(&user.usdc), 100 * USDC);
}

/// A reference model of the limits, run against the program on random
/// mandates, pulls and waits: the program accepts exactly what the model
/// accepts, so no sequence of pulls gets past any limit.
#[test]
fn random_pulls_never_pass_a_limit() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut random = |below: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % below
    };

    let mut f = Fixture::new();
    let user = f.wallet(u64::MAX / 2, 0);
    let merchant = f.wallet(0, 0);
    let (u, m, usdc) = (
        user.address().to_bytes(),
        merchant.address().to_bytes(),
        f.usdc.to_bytes(),
    );
    let user_usdc = user.usdc.to_bytes();
    let (mut now, mut paid) = (NOW, 0u64);
    let (mut accepted_count, mut refused_count) = (0, 0);

    for salt in 0..150 {
        // One to three stacked limits; the first one always persists
        let count = 1 + random(3) as usize;
        let limits: Vec<_> = (0..count)
            .map(|k| {
                let per = match if k == 0 { random(2) } else { random(3) } {
                    0 => Per::Total,
                    1 => Per::Every(1 + random(1_000) as u32),
                    _ => Per::Use,
                };
                limit(&user_usdc, &usdc, 1 + random(1_000), per)
            })
            .collect();
        let mut terms = terms(&u, Some(&m), None, &limits, None);
        terms.salt = salt;
        let bytes = encode(&terms);
        let create = create(&user.address(), &user.address(), &bytes, None);
        f.send(&[create], &[&user.key]).unwrap();

        let (mut consumed, mut rolled) = (vec![0u64; count], 0i64);
        for _ in 0..25 {
            now += random(600) as i64;
            f.set_time(now);
            let amount = random(1_200);

            // The model: what each limit has spent now, and whether this pull fits all of them
            let spent: Vec<u64> = (limits.iter().zip(&consumed))
                .map(|(l, consumed)| match l.per {
                    Per::Total => *consumed,
                    Per::Every(seconds) => {
                        let window = |t: i64| (t - NOW).div_euclid(seconds as i64);
                        if window(now) == window(rolled) {
                            *consumed
                        } else {
                            0
                        }
                    }
                    Per::Use => 0,
                })
                .collect();
            let fits = limits.iter().zip(&spent).all(|(l, s)| s + amount <= l.max);

            let accounts = (user.usdc, f.usdc, merchant.usdc);
            let spender = merchant.address();
            let ix = pull(
                &spender,
                &user.address(),
                &bytes,
                accounts,
                amount,
                None,
                TOKEN,
            );
            let accepted = f.send(&[ix], &[&merchant.key]).is_ok();
            assert_eq!(accepted, fits, "mandate {salt}: {limits:?} pull {amount}");

            if accepted {
                paid += amount;
                consumed = spent.iter().map(|s| s + amount).collect();
                rolled = now;
                accepted_count += 1;
            } else {
                refused_count += 1;
            }
        }
    }
    assert_eq!(f.balance(&merchant.usdc), paid);
    // The run must exercise both outcomes to mean anything
    assert!(accepted_count > 200 && refused_count > 200);
}

#[test]
fn engine_constant_is_the_derived_pda() {
    assert_eq!(pda(&[ENGINE_SEED]), ENGINE_KEY);
    let (_, bump) = Address::find_program_address(&[ENGINE_SEED], &PROGRAM);
    assert_eq!(bump, ENGINE_BUMP);
}
