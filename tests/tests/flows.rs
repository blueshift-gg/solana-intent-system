use mandate_core::constants::*;
use mandate_core::errors::MandateError;
use mandate_core::terms::{Bound, Refill};
use mandate_tests::*;
use solana_address::Address;
use solana_signer::Signer;

const DAY: i64 = 86_400;
const MONTHLY: Refill = Refill::Over {
    period: 30 * DAY as u32,
};

#[test]
fn signed_swap_intent_fills_through_any_solver() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let solver = f.wallet(0, 10 * SOL);
    let (u, usdc, sol) = (
        user.address().to_bytes(),
        f.usdc.to_bytes(),
        f.sol.to_bytes(),
    );
    let (user_usdc, user_sol) = (user.usdc.to_bytes(), user.sol.to_bytes());

    // At most 100 USDC leaves; at least 0.52 SOL arrives, decaying to 0.50 over five minutes
    let takes = [take(&user_usdc, &usdc, 100 * USDC, Refill::Never)];
    let decay = Bound::Linear {
        t0: NOW,
        v0: 520_000_000,
        t1: NOW + 300,
        v1: 500_000_000,
    };
    let requires = [gain(&user_sol, &sol, &u, decay)];
    let terms = terms(&u, true, Some(NOW + 300), &takes, &requires);
    let bytes = encode(&terms);
    let signature = sign(&terms, &user.key, f.decimals());

    // Halfway down the auction the solver brings the signature and fills at
    // 0.51 SOL. The user signs no transaction and pays nothing
    f.set_time(NOW + 150);
    let extra = extra(&[user.usdc, solver.usdc, user.sol], &[f.usdc, f.sol, TOKEN]);
    let (s, mints) = (solver.address(), [f.usdc, f.sol]);
    let open = open(
        &s,
        &s,
        &user.address(),
        &bytes,
        extra,
        &[(user.usdc, solver.usdc, 100 * USDC)],
    );
    let fill = [
        create_mandate(&user.address(), &s, &bytes, Some((&signature, &mints))),
        open.clone(),
        transfer(&solver.sol, &f.sol, &user.sol, &s, 510_000_000, 9),
        close(&s, &[user.usdc, user.sol]),
    ];
    f.send(&fill, &[&solver.key]).unwrap();

    assert_eq!(f.balance(&user.usdc), 0);
    assert_eq!(f.balance(&user.sol), 510_000_000);
    assert_eq!(f.balance(&solver.usdc), 100 * USDC);

    // The mandate is its own tombstone: no second fill, the signature cannot
    // create it again, and once it has expired the solver gets the rent back
    let mandate = mandate_pda(&user.address(), &bytes);
    let reclaim = close_mandate(&mandate, &s, &user.address());
    assert!(f.send(&fill, &[&solver.key]).is_err());
    assert!(f.send(&[open, fill[3].clone()], &[&solver.key]).is_err());
    assert!(f
        .send(std::slice::from_ref(&reclaim), &[&solver.key])
        .is_err());
    f.set_time(NOW + 300);
    f.send(&[reclaim], &[&solver.key]).unwrap();
    assert!(f.svm.get_account(&mandate).is_none_or(|a| a.lamports == 0));
}

#[test]
fn subscription_is_one_instruction_and_refills_over_its_period() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let (merchant, other) = (f.wallet(0, 0), f.wallet(0, 0));
    let (u, usdc) = (user.address().to_bytes(), f.usdc.to_bytes());
    let (user_usdc, merchant_usdc) = (user.usdc.to_bytes(), [merchant.usdc.to_bytes()]);

    // At most 10 USDC per 30 days, and only to the merchant
    let takes = [pay(&user_usdc, &usdc, 10 * USDC, MONTHLY, &merchant_usdc)];
    let bytes = encode(&terms(&u, false, None, &takes, &[]));
    let create = create_mandate(&user.address(), &user.address(), &bytes, None);
    f.send(&[create], &[&user.key]).unwrap();

    // Anyone collects, with one instruction and no session. The budget refills
    // linearly: half of it is back after half the period, never all of it early
    let collector = f.payer();
    let pull = |f: &mut Fixture, day: i64, to: Address, amount: u64| {
        f.set_time(NOW + day * DAY);
        let c = collector.pubkey();
        let extra = extra(&[user.usdc, to], &[f.usdc, TOKEN]);
        let pull = [(user.usdc, to, amount)];
        let open = open(&c, &c, &user.address(), &bytes, extra, &pull);
        f.send(&[open], &[&collector]).is_ok()
    };
    assert!(!pull(&mut f, 0, other.usdc, USDC));
    assert!(pull(&mut f, 0, merchant.usdc, 10 * USDC));
    assert!(!pull(&mut f, 15, merchant.usdc, 5 * USDC + 1));
    assert!(pull(&mut f, 15, merchant.usdc, 5 * USDC));
    assert!(!pull(&mut f, 30, merchant.usdc, 5 * USDC + 1));
    assert!(pull(&mut f, 30, merchant.usdc, 5 * USDC));

    assert_eq!(f.balance(&user.usdc), 80 * USDC);
    assert_eq!(f.balance(&merchant.usdc), 20 * USDC);
}

#[test]
fn a_program_collects_a_subscription_by_cpi() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let merchant = f.wallet(0, 0);
    let (u, usdc) = (user.address().to_bytes(), f.usdc.to_bytes());
    let (user_usdc, merchant_usdc) = (user.usdc.to_bytes(), [merchant.usdc.to_bytes()]);

    let takes = [pay(&user_usdc, &usdc, 10 * USDC, MONTHLY, &merchant_usdc)];
    let bytes = encode(&terms(&u, false, None, &takes, &[]));
    let create = create_mandate(&user.address(), &user.address(), &bytes, None);
    f.send(&[create], &[&user.key]).unwrap();

    // The merchant's own program makes the pull, as it would while renewing a membership
    let m = merchant.address();
    let extra = extra(&[user.usdc, merchant.usdc], &[f.usdc, TOKEN]);
    let pull = [(user.usdc, merchant.usdc, 10 * USDC)];
    let open = by_cpi(open(&m, &m, &user.address(), &bytes, extra, &pull));
    f.send(std::slice::from_ref(&open), &[&merchant.key])
        .unwrap();
    assert!(f.send(&[open], &[&merchant.key]).is_err());

    assert_eq!(f.balance(&merchant.usdc), 10 * USDC);
}

#[test]
fn signed_subscription_is_created_by_its_first_pull() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let merchant = f.wallet(0, 0);
    let (u, usdc) = (user.address().to_bytes(), f.usdc.to_bytes());
    let (user_usdc, merchant_usdc) = (user.usdc.to_bytes(), [merchant.usdc.to_bytes()]);

    // Signed once, off chain: at most 10 USDC per 30 days, only to the merchant
    let takes = [pay(&user_usdc, &usdc, 10 * USDC, MONTHLY, &merchant_usdc)];
    let terms = terms(&u, false, None, &takes, &[]);
    let bytes = encode(&terms);
    let signature = sign(&terms, &user.key, f.decimals());

    // The merchant's first pull brings the signature; later pulls are the
    // same single instruction a created mandate takes
    let m = merchant.address();
    let extra = extra(&[user.usdc, merchant.usdc], &[f.usdc, TOKEN]);
    let pull = [(user.usdc, merchant.usdc, 10 * USDC)];
    let open = open(&m, &m, &user.address(), &bytes, extra, &pull);
    let create = create_mandate(&user.address(), &m, &bytes, Some((&signature, &[f.usdc])));
    f.send(&[create, open.clone()], &[&merchant.key]).unwrap();
    f.set_time(NOW + 30 * DAY);
    f.send(&[open], &[&merchant.key]).unwrap();

    assert_eq!(f.balance(&user.usdc), 80 * USDC);
    assert_eq!(f.balance(&merchant.usdc), 20 * USDC);
}

#[test]
fn limits_stack_on_one_account() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let agent = f.wallet(0, 0);
    let (u, usdc, user_usdc) = (
        user.address().to_bytes(),
        f.usdc.to_bytes(),
        user.usdc.to_bytes(),
    );
    let a = agent.address();

    // An agent's key may spend anywhere: at most 5 USDC per use, 12 a day
    let daily = Refill::Over { period: DAY as u32 };
    let takes = [
        take(&user_usdc, &usdc, 12 * USDC, daily),
        take(&user_usdc, &usdc, 5 * USDC, Refill::EachUse),
    ];
    let mut terms = terms(&u, false, Some(NOW + 30 * DAY), &takes, &[]);
    let agent_key = a.to_bytes();
    terms.executor = Some(&agent_key);
    let bytes = encode(&terms);
    let create = create_mandate(&user.address(), &user.address(), &bytes, None);
    f.send(&[create], &[&user.key]).unwrap();

    let spend = |f: &mut Fixture, amount: u64| {
        let extra = extra(&[user.usdc, agent.usdc], &[f.usdc, TOKEN]);
        let pull = [(user.usdc, agent.usdc, amount)];
        let open = open(&a, &a, &user.address(), &bytes, extra, &pull);
        f.send(&[open], &[&agent.key]).is_ok()
    };
    assert!(!spend(&mut f, 5 * USDC + 1));
    assert!(spend(&mut f, 5 * USDC));
    assert!(spend(&mut f, 5 * USDC));
    assert!(!spend(&mut f, 2 * USDC + 1));
    assert!(spend(&mut f, 2 * USDC));
    assert_eq!(f.balance(&agent.usdc), 12 * USDC);
}

#[test]
fn two_mandates_net_against_each_other_without_outside_liquidity() {
    let mut f = Fixture::new();
    let alice = f.wallet(100 * USDC, 0);
    let bob = f.wallet(0, SOL);
    let solver = f.payer();
    let (usdc, sol) = (f.usdc.to_bytes(), f.sol.to_bytes());
    let (a, a_usdc, a_sol) = (
        alice.address().to_bytes(),
        alice.usdc.to_bytes(),
        alice.sol.to_bytes(),
    );
    let (b, b_usdc, b_sol) = (
        bob.address().to_bytes(),
        bob.usdc.to_bytes(),
        bob.sol.to_bytes(),
    );

    // Alice sells 100 USDC for at least 1 SOL; Bob sells 1 SOL for at least 100 USDC
    let alice_takes = [take(&a_usdc, &usdc, 100 * USDC, Refill::Never)];
    let alice_requires = [gain(&a_sol, &sol, &a, Bound::Const(SOL))];
    let bob_takes = [take(&b_sol, &sol, SOL, Refill::Never)];
    let bob_requires = [gain(&b_usdc, &usdc, &b, Bound::Const(100 * USDC))];
    let alice_bytes = encode(&terms(&a, true, None, &alice_takes, &alice_requires));
    let bob_bytes = encode(&terms(&b, true, None, &bob_takes, &bob_requires));
    for (wallet, bytes) in [(&alice, &alice_bytes), (&bob, &bob_bytes)] {
        let create = create_mandate(&wallet.address(), &wallet.address(), bytes, None);
        f.send(&[create], &[&wallet.key]).unwrap();
    }

    // The solver hands each side the other's tokens
    let accounts = [alice.usdc, alice.sol, bob.usdc, bob.sol];
    let s = solver.pubkey();
    let settle = [
        open(
            &s,
            &s,
            &alice.address(),
            &alice_bytes,
            extra(&accounts, &[f.usdc, TOKEN]),
            &[(alice.usdc, bob.usdc, 100 * USDC)],
        ),
        open(
            &s,
            &s,
            &bob.address(),
            &bob_bytes,
            extra(&accounts, &[f.sol, TOKEN]),
            &[(bob.sol, alice.sol, SOL)],
        ),
        close(&s, &accounts),
    ];
    f.send(&settle, &[&solver]).unwrap();

    assert_eq!((f.balance(&alice.usdc), f.balance(&alice.sol)), (0, SOL));
    assert_eq!((f.balance(&bob.usdc), f.balance(&bob.sol)), (100 * USDC, 0));
}

/// What a payment pays into an account is owed to it already, so it must not
/// count toward another mandate's requirement on that account.
#[test]
fn a_payment_cannot_fill_another_mandates_requirement() {
    let mut f = Fixture::new();
    let seller = f.wallet(0, SOL);
    let subscriber = f.wallet(100 * USDC, 0);
    let thief = f.wallet(0, 0);
    let (usdc, sol) = (f.usdc.to_bytes(), f.sol.to_bytes());
    let (s, s_sol, s_usdc) = (
        seller.address().to_bytes(),
        seller.sol.to_bytes(),
        [seller.usdc.to_bytes()],
    );
    let (p, p_usdc) = (subscriber.address().to_bytes(), subscriber.usdc.to_bytes());

    // The seller sells 1 SOL for at least 10 USDC, and is owed 10 USDC a month
    let sale_takes = [take(&s_sol, &sol, SOL, Refill::Never)];
    let sale_requires = [gain(&s_usdc[0], &usdc, &s, Bound::Const(10 * USDC))];
    let sale = encode(&terms(&s, true, None, &sale_takes, &sale_requires));
    let dues_takes = [pay(&p_usdc, &usdc, 10 * USDC, MONTHLY, &s_usdc)];
    let dues = encode(&terms(&p, false, None, &dues_takes, &[]));
    for (wallet, bytes) in [(&seller, &sale), (&subscriber, &dues)] {
        let create = create_mandate(&wallet.address(), &wallet.address(), bytes, None);
        f.send(&[create], &[&wallet.key]).unwrap();
    }

    // Taking the SOL and paying for it with the seller's own dues fails
    let t = thief.address();
    let accounts = [seller.sol, thief.sol, subscriber.usdc, seller.usdc];
    let steal = [
        open(
            &t,
            &t,
            &seller.address(),
            &sale,
            extra(&accounts, &[f.sol, f.usdc, TOKEN]),
            &[(seller.sol, thief.sol, SOL)],
        ),
        open(
            &t,
            &t,
            &subscriber.address(),
            &dues,
            extra(&accounts, &[f.usdc, TOKEN]),
            &[(subscriber.usdc, seller.usdc, 10 * USDC)],
        ),
        close(&t, &[seller.sol, seller.usdc]),
    ];
    let refused = format!("Custom({})", MandateError::PaymentInSession as u32);
    let error = f.send(&steal, &[&thief.key]).unwrap_err().err;
    assert!(format!("{error:?}").contains(&refused), "{error:?}");

    // Nor does hiding the payment inside another program
    let payment = open(
        &t,
        &t,
        &subscriber.address(),
        &dues,
        extra(&[subscriber.usdc, seller.usdc], &[f.usdc, TOKEN]),
        &[(subscriber.usdc, seller.usdc, 10 * USDC)],
    );
    let hidden = [steal[0].clone(), by_cpi(payment), steal[2].clone()];
    let error = f.send(&hidden, &[&thief.key]).unwrap_err().err;
    assert!(format!("{error:?}").contains(&refused), "{error:?}");
}

#[test]
fn authority_revokes_and_reclaims_everything() {
    let mut f = Fixture::new();
    let user = f.wallet(10 * USDC, 0);
    let merchant = f.wallet(0, 0);
    let (u, usdc, user_usdc) = (
        user.address().to_bytes(),
        f.usdc.to_bytes(),
        user.usdc.to_bytes(),
    );
    let to = [merchant.usdc.to_bytes()];
    let key = user.address();
    let epoch = pda(&[EPOCH_SEED, key.as_ref()]);

    // A mandate the authority created, then revoked. It was also signed once:
    // the account stays, so that signature cannot create the mandate again
    let takes = [pay(&user_usdc, &usdc, USDC, Refill::Never, &to)];
    let created = terms(&u, false, None, &takes, &[]);
    let created_signature = sign(&created, &user.key, f.decimals());
    let created = encode(&created);
    f.send(&[create_mandate(&key, &key, &created, None)], &[&user.key])
        .unwrap();

    // A mandate that was only signed is revoked by its id, used or not
    let takes = [pay(&user_usdc, &usdc, 2 * USDC, Refill::Never, &to)];
    let signed = terms(&u, false, None, &takes, &[]);
    let signed_signature = sign(&signed, &user.key, f.decimals());
    let signed = encode(&signed);

    // Terms signed for an epoch still to come: no bump may ever make them valid
    let mut early = terms(&u, false, None, &takes, &[]);
    early.epoch = 1;
    let early_signature = sign(&early, &user.key, f.decimals());
    let early = encode(&early);

    let mut closes = Vec::new();
    for (bytes, signature) in [(&created, created_signature), (&signed, signed_signature)] {
        let mandate = mandate_pda(&key, bytes);
        let revoke = authority_ix(1, &key, &mandate, &[epoch], &mandate_id(bytes));
        f.send(&[revoke], &[&user.key]).unwrap();
        let create = create_mandate(&key, &key, bytes, Some((&signature, &[f.usdc])));
        assert!(f.send(&[create], &[&user.key]).is_err());
        let close = close_mandate(&mandate, &key, &key);
        assert!(f.send(std::slice::from_ref(&close), &[&user.key]).is_err());
        closes.push((mandate, close));
    }

    // Both stay until their terms can never run again, here once the
    // authority bumps its epoch. Then the rent comes back
    f.send(&[authority_ix(2, &key, &epoch, &[], &[])], &[&user.key])
        .unwrap();
    for (mandate, close) in closes {
        f.send(&[close], &[&user.key]).unwrap();
        assert!(f.svm.get_account(&mandate).is_none_or(|a| a.lamports == 0));
    }
    let create = create_mandate(&key, &key, &early, Some((&early_signature, &[f.usdc])));
    assert!(f.send(&[create], &[&user.key]).is_err());
}

#[test]
fn the_named_executor_can_revoke_its_own_mandate() {
    let mut f = Fixture::new();
    let user = f.wallet(10 * USDC, 0);
    let (merchant, stranger) = (f.wallet(0, 0), f.payer());
    let (u, usdc, user_usdc) = (
        user.address().to_bytes(),
        f.usdc.to_bytes(),
        user.usdc.to_bytes(),
    );
    let (key, m) = (user.address(), merchant.address());

    // The merchant is the only executor, and sends the tokens where it likes
    let takes = [take(&user_usdc, &usdc, USDC, MONTHLY)];
    let spender = m.to_bytes();
    let mut terms = terms(&u, false, None, &takes, &[]);
    terms.executor = Some(&spender);
    let bytes = encode(&terms);
    f.send(&[create_mandate(&key, &key, &bytes, None)], &[&user.key])
        .unwrap();

    // A stranger cannot revoke it; the merchant can, and then cannot pull
    let (mandate, epoch) = (mandate_pda(&key, &bytes), pda(&[EPOCH_SEED, key.as_ref()]));
    let id = mandate_id(&bytes);
    let revoke = authority_ix(1, &stranger.pubkey(), &mandate, &[epoch], &id);
    assert!(f.send(&[revoke], &[&stranger]).is_err());
    let revoke = authority_ix(1, &m, &mandate, &[epoch], &id);
    f.send(&[revoke], &[&merchant.key]).unwrap();

    let extra = extra(&[user.usdc, merchant.usdc], &[f.usdc, TOKEN]);
    let pull = [(user.usdc, merchant.usdc, USDC)];
    let open = open(&m, &m, &key, &bytes, extra, &pull);
    assert!(f.send(&[open], &[&merchant.key]).is_err());
}

#[test]
fn one_enable_serves_many_approvals_under_one_budget() {
    let mut f = Fixture::new();
    let user = f.wallet(100 * USDC, 0);
    let (solver, merchant, api) = (f.wallet(0, SOL), f.wallet(0, 0), f.wallet(0, 0));
    let (u, usdc, sol) = (
        user.address().to_bytes(),
        f.usdc.to_bytes(),
        f.sol.to_bytes(),
    );
    let (user_usdc, user_sol) = (user.usdc.to_bytes(), user.sol.to_bytes());
    let (merchant_usdc, api_usdc) = ([merchant.usdc.to_bytes()], [api.usdc.to_bytes()]);
    let user_key = user.address();

    // Setup, once: every mandate on this USDC account shares a 50 USDC budget
    f.enable(&user, &user.usdc, 50 * USDC);

    // Approval 1, signed: swap at most 20 USDC for at least 0.2 SOL
    let swap_takes = [take(&user_usdc, &usdc, 20 * USDC, Refill::Never)];
    let swap_requires = [gain(&user_sol, &sol, &u, Bound::Const(SOL / 5))];
    let swap = terms(&u, true, Some(NOW + DAY), &swap_takes, &swap_requires);
    let swap_signature = sign(&swap, &user.key, f.decimals());
    let swap = encode(&swap);

    // Approval 2, created: at most 10 USDC a month to the merchant
    let subscription_takes = [pay(&user_usdc, &usdc, 10 * USDC, MONTHLY, &merchant_usdc)];
    let subscription = encode(&terms(&u, false, None, &subscription_takes, &[]));
    let create = create_mandate(&user_key, &user_key, &subscription, None);
    f.send(&[create], &[&user.key]).unwrap();

    // Approval 3, signed: at most 5 USDC a day to the API
    let daily = Refill::Over { period: DAY as u32 };
    let agent_takes = [pay(&user_usdc, &usdc, 5 * USDC, daily, &api_usdc)];
    let agent = terms(&u, false, None, &agent_takes, &[]);
    let agent_signature = sign(&agent, &user.key, f.decimals());
    let agent = encode(&agent);

    // Three executors, three different things, no further action from the user
    let s = solver.address();
    let fill = [
        create_mandate(
            &user_key,
            &s,
            &swap,
            Some((&swap_signature, &[f.usdc, f.sol])),
        ),
        open(
            &s,
            &s,
            &user_key,
            &swap,
            extra(&[user.usdc, solver.usdc, user.sol], &[f.usdc, f.sol, TOKEN]),
            &[(user.usdc, solver.usdc, 20 * USDC)],
        ),
        transfer(&solver.sol, &f.sol, &user.sol, &s, SOL / 5, 9),
        close(&s, &[user.usdc, user.sol]),
    ];
    f.send(&fill, &[&solver.key]).unwrap();

    let m = merchant.address();
    let pull = open(
        &m,
        &m,
        &user_key,
        &subscription,
        extra(&[user.usdc, merchant.usdc], &[f.usdc, TOKEN]),
        &[(user.usdc, merchant.usdc, 10 * USDC)],
    );
    f.send(&[pull], &[&merchant.key]).unwrap();

    let a = api.address();
    let charge = [
        create_mandate(&user_key, &a, &agent, Some((&agent_signature, &[f.usdc]))),
        open(
            &a,
            &a,
            &user_key,
            &agent,
            extra(&[user.usdc, api.usdc], &[f.usdc, TOKEN]),
            &[(user.usdc, api.usdc, 5 * USDC)],
        ),
    ];
    f.send(&charge, &[&api.key]).unwrap();

    // Each approval took its own share, and all of them drew on the one budget
    assert_eq!(f.balance(&user.usdc), 65 * USDC);
    assert_eq!(f.balance(&user.sol), SOL / 5);
    assert_eq!(
        (f.balance(&merchant.usdc), f.balance(&api.usdc)),
        (10 * USDC, 5 * USDC)
    );
    assert_eq!(f.allowance(&user.usdc), 15 * USDC);
}

/// A Token-2022 transfer fee comes out of what arrives, never out of the
/// payer on top of the limit.
#[test]
fn a_fee_token_never_takes_more_than_the_limit() {
    use spl_token_2022_interface::extension::transfer_fee::instruction::initialize_transfer_fee_config;
    use spl_token_2022_interface::extension::ExtensionType;
    use spl_token_2022_interface::instruction::{approve, initialize_mint2, mint_to};
    use spl_token_2022_interface::{state::Mint, ID as TOKEN_2022};

    let mut f = Fixture::new();
    let (user, merchant, issuer) = (f.payer(), f.payer(), f.payer());

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
        litesvm_token::CreateAssociatedTokenAccount::new(&mut f.svm, owner, &mint)
            .token_program_id(&TOKEN_2022)
            .send()
            .unwrap()
    };
    let (from, to) = (account(&user), account(&merchant));
    let fund = mint_to(&TOKEN_2022, &mint, &from, &issuer_key, &[], 100 * USDC).unwrap();
    f.send(&[fund], &[&issuer]).unwrap();
    let enable = approve(
        &TOKEN_2022,
        &from,
        &ENGINE_KEY,
        &user.pubkey(),
        &[],
        u64::MAX,
    )
    .unwrap();
    f.send(&[enable], &[&user]).unwrap();
    let held = |f: &Fixture, account: &Address| {
        let data = f.svm.get_account(account).unwrap().data;
        u64::from_le_bytes(data[64..72].try_into().unwrap())
    };

    // At most 10 a month, only to the merchant
    let (u, m, from_key, to_key) = (
        user.pubkey().to_bytes(),
        mint.to_bytes(),
        from.to_bytes(),
        [to.to_bytes()],
    );
    let takes = [pay(&from_key, &m, 10 * USDC, MONTHLY, &to_key)];
    let bytes = encode(&terms(&u, false, None, &takes, &[]));
    let create = create_mandate(&user.pubkey(), &user.pubkey(), &bytes, None);
    f.send(&[create], &[&user]).unwrap();

    let pull = |f: &mut Fixture, amount: u64| {
        let k = merchant.pubkey();
        let extra = extra(&[from, to], &[mint, TOKEN_2022]);
        let open = open(&k, &k, &user.pubkey(), &bytes, extra, &[(from, to, amount)]);
        f.send(&[open], &[&merchant]).is_ok()
    };
    assert!(pull(&mut f, 10 * USDC));
    assert!(!pull(&mut f, 1));

    // The payer gave up exactly the limit; the merchant received it less the fee
    assert_eq!(held(&f, &from), 90 * USDC);
    assert_eq!(held(&f, &to), 10 * USDC - USDC / 10);
}

/// A reference model of the limits, run against the program on random
/// mandates, pulls and waits: the program accepts exactly what the model
/// accepts, so no sequence of pulls gets past any take.
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
    let (u, usdc, user_usdc) = (
        user.address().to_bytes(),
        f.usdc.to_bytes(),
        user.usdc.to_bytes(),
    );
    let (mut now, mut paid) = (NOW, 0u64);

    for scenario in 0..150 {
        // One to three stacked limits; the first one always persists
        let count = 1 + random(3) as usize;
        let takes: Vec<_> = (0..count)
            .map(|k| {
                let refill = match if k == 0 { random(2) } else { random(3) } {
                    0 => Refill::Never,
                    1 => Refill::Over {
                        period: 1 + random(1_000) as u32,
                    },
                    _ => Refill::EachUse,
                };
                take(&user_usdc, &usdc, 1 + random(1_000), refill)
            })
            .collect();
        // A distinct expiry makes each scenario a distinct mandate
        let expiry = Some(NOW + 10_000_000 + scenario);
        let spender = merchant.address().to_bytes();
        let mut terms = terms(&u, false, expiry, &takes, &[]);
        terms.executor = Some(&spender);
        let bytes = encode(&terms);
        let create = create_mandate(&user.address(), &user.address(), &bytes, None);
        f.send(&[create], &[&user.key]).unwrap();

        let (mut consumed, mut rolled, mut done) = (vec![0u64; count], 0i64, false);
        for _ in 0..25 {
            now += random(600) as i64;
            f.set_time(now);
            let amount = random(1_200);

            // The model: what each take has spent now, and whether this pull fits all of them
            let spent: Vec<u64> = (takes.iter().zip(&consumed))
                .map(|(t, consumed)| match t.refill {
                    Refill::Never => *consumed,
                    Refill::Over { period } => {
                        let elapsed = (now - rolled).clamp(0, period as i64) as u128;
                        consumed.saturating_sub((t.max as u128 * elapsed / period as u128) as u64)
                    }
                    Refill::EachUse => 0,
                })
                .collect();
            let fits = !done && takes.iter().zip(&spent).all(|(t, s)| s + amount <= t.max);

            let m = merchant.address();
            let extra = extra(&[user.usdc, merchant.usdc], &[f.usdc, TOKEN]);
            let pull = [(user.usdc, merchant.usdc, amount)];
            let open = open(&m, &m, &user.address(), &bytes, extra, &pull);
            let accepted = f.send(&[open], &[&merchant.key]).is_ok();
            assert_eq!(
                accepted, fits,
                "scenario {scenario}: {takes:?} pull {amount}"
            );

            if accepted && amount > 0 {
                paid += amount;
                consumed = spent.iter().map(|s| s + amount).collect();
                rolled = now;
                done = (takes.iter().zip(&consumed))
                    .any(|(t, c)| t.refill == Refill::Never && *c == t.max);
            }
        }
    }
    assert_eq!(f.balance(&merchant.usdc), paid);
}

/// A reference model of the session, run against the program on random
/// pairs of exchanges settled in one transaction: pulls go to the counterparty
/// or to the solver, the solver adds what it likes, and the second mandate
/// either nets against the first or requires the very same account. `Close`
/// passes exactly when every account gained the sum of what was required of it.
#[test]
fn random_settlements_pass_exactly_when_every_requirement_is_met() {
    use std::collections::HashMap;

    let mut seed = 0xD1B5_4A32_D192_ED03u64;
    let mut random = |below: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % below
    };

    let mut f = Fixture::new();
    let (alice, bob) = (f.wallet(1 << 40, 1 << 40), f.wallet(1 << 40, 1 << 40));
    let solver = f.wallet(1 << 40, 1 << 40);
    let (usdc, sol) = (f.usdc.to_bytes(), f.sol.to_bytes());
    let (a, b) = (alice.address().to_bytes(), bob.address().to_bytes());
    let (a_usdc, a_sol) = (alice.usdc.to_bytes(), alice.sol.to_bytes());
    let (b_usdc, b_sol) = (bob.usdc.to_bytes(), bob.sol.to_bytes());
    let s = solver.address();
    let users = [alice.usdc, alice.sol, bob.usdc, bob.sol];
    let (mut passed, mut failed) = (0, 0);

    for scenario in 0..200 {
        // Alice sells USDC for SOL. Bob either sells SOL for USDC, or sells
        // USDC and requires Alice's SOL account to gain as well
        let shared = random(2) == 0;
        let (alice_max, bob_max) = (1 + random(1_000), 1 + random(1_000));
        let (alice_wants, bob_wants) = (1 + random(1_000), 1 + random(1_000));
        let alice_takes = [take(&a_usdc, &usdc, alice_max, Refill::Never)];
        let alice_requires = [gain(&a_sol, &sol, &a, Bound::Const(alice_wants))];
        let bob_takes = match shared {
            true => [take(&b_usdc, &usdc, bob_max, Refill::Never)],
            false => [take(&b_sol, &sol, bob_max, Refill::Never)],
        };
        let bob_requires = match shared {
            true => [gain(&a_sol, &sol, &a, Bound::Const(bob_wants))],
            false => [gain(&b_usdc, &usdc, &b, Bound::Const(bob_wants))],
        };
        // A distinct expiry makes each scenario a distinct pair of mandates
        let expiry = Some(NOW + 10_000_000 + scenario);
        let alice_bytes = encode(&terms(&a, true, expiry, &alice_takes, &alice_requires));
        let bob_bytes = encode(&terms(&b, true, expiry, &bob_takes, &bob_requires));
        for (wallet, bytes) in [(&alice, &alice_bytes), (&bob, &bob_bytes)] {
            let create = create_mandate(&wallet.address(), &wallet.address(), bytes, None);
            f.send(&[create], &[&wallet.key]).unwrap();
        }

        // Each pull goes to the counterparty or to the solver, and may exceed its limit
        let (alice_pull, bob_pull) = (random(alice_max + 100), random(bob_max + 100));
        let alice_to = [bob.usdc, solver.usdc][random(2) as usize];
        let (bob_from, bob_to) = match shared {
            true => (bob.usdc, [alice.usdc, solver.usdc][random(2) as usize]),
            false => (bob.sol, [alice.sol, solver.sol][random(2) as usize]),
        };
        // The solver tops up what the mandates require, sometimes short
        let (to_alice_sol, to_bob_usdc) = (random(2_200), random(1_100));

        // The model: every account's change, against the sum required of it
        let mut change: HashMap<Address, i128> = HashMap::new();
        let mut required: HashMap<Address, i128> = HashMap::new();
        for (from, to, amount) in [
            (alice.usdc, alice_to, alice_pull),
            (bob_from, bob_to, bob_pull),
        ] {
            *change.entry(from).or_default() -= amount as i128;
            *change.entry(to).or_default() += amount as i128;
            *required.entry(from).or_default() -= amount as i128;
        }
        *change.entry(alice.sol).or_default() += to_alice_sol as i128;
        *change.entry(bob.usdc).or_default() += to_bob_usdc as i128;
        *required.entry(alice.sol).or_default() += alice_wants as i128;
        let bob_target = if shared { alice.sol } else { bob.usdc };
        *required.entry(bob_target).or_default() += bob_wants as i128;
        let within = alice_pull <= alice_max && bob_pull <= bob_max;
        let met = (required.iter()).all(|(k, r)| change.get(k).copied().unwrap_or(0) >= *r);

        let accounts = [users.as_slice(), &[solver.usdc, solver.sol]].concat();
        let extra = || extra(&accounts, &[f.usdc, f.sol, TOKEN]);
        let settle = [
            open(
                &s,
                &s,
                &alice.address(),
                &alice_bytes,
                extra(),
                &[(alice.usdc, alice_to, alice_pull)],
            ),
            open(
                &s,
                &s,
                &bob.address(),
                &bob_bytes,
                extra(),
                &[(bob_from, bob_to, bob_pull)],
            ),
            transfer(&solver.sol, &f.sol, &alice.sol, &s, to_alice_sol, 9),
            transfer(&solver.usdc, &f.usdc, &bob.usdc, &s, to_bob_usdc, 6),
            close(&s, &users),
        ];
        let accepted = f.send(&settle, &[&solver.key]).is_ok();
        assert_eq!(
            accepted,
            within && met,
            "scenario {scenario}, shared {shared}"
        );
        match accepted {
            true => passed += 1,
            false => failed += 1,
        }
    }
    // The run must exercise both outcomes to mean anything
    assert!(
        passed > 20 && failed > 20,
        "{passed} passed, {failed} failed"
    );
}

#[test]
fn engine_constant_is_the_derived_pda() {
    assert_eq!(pda(&[ENGINE_SEED]), ENGINE_KEY);
    let (_, bump) = solana_address::Address::find_program_address(&[ENGINE_SEED], &PROGRAM);
    assert_eq!(bump, ENGINE_BUMP);
}
