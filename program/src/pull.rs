use crate::events::emit;
use crate::helpers::{balance, transfer};
use crate::state::policy;
use mandate_core::errors::MandateError;
use mandate_core::terms::Terms;
use pinocchio::log::sol_log;
use pinocchio::sysvars::{clock::Clock, Sysvar};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// The accounts a pull moves tokens between, shared by `Pull` and `Fill`.
pub struct Legs<'a> {
    pub spender: &'a AccountInfo,
    pub from: &'a AccountInfo,
    pub mint: &'a AccountInfo,
    pub to: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    /// `[pay_from, pay_mint, pay_to]` when the terms have a price.
    pub payment: &'a [AccountInfo],
}

impl Legs<'_> {
    /// Only inside the window, and only by the spender the terms name.
    pub fn check(&self, terms: &Terms, now: i64) -> ProgramResult {
        if now < terms.not_before {
            return Err(MandateError::NotYetValid.into());
        }
        if terms.not_after.is_some_and(|t| now >= t) {
            return Err(MandateError::Expired.into());
        }
        if terms.spender.is_some_and(|s| s.ne(self.spender.key())) {
            return Err(MandateError::InvalidSpender.into());
        }
        Ok(())
    }

    /// Take `amount` as the delegate, only from the authority's own account.
    /// If the terms have a price the spender pays it, and the authority must
    /// receive all of it. Returns what was paid.
    pub fn settle(&self, terms: &Terms, amount: u64, now: i64) -> Result<u64, ProgramError> {
        balance(self.from, self.mint.key(), terms.authority)?;
        transfer(self.from, self.mint, self.to, self.engine, amount)?;

        let Some(price) = terms.price else {
            return Ok(0);
        };
        let [pay_from, pay_mint, pay_to, ..] = self.payment else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        if price.to.ne(pay_to.key()) || price.mint.ne(pay_mint.key()) {
            return Err(MandateError::InvalidTarget.into());
        }
        let due = price.due(amount, now)?;
        let before = balance(pay_to, price.mint, terms.authority)?;
        transfer(pay_from, pay_mint, pay_to, self.spender, due)?;
        let after = balance(pay_to, price.mint, terms.authority)?;
        if after.checked_sub(before).is_none_or(|got| got < due) {
            return Err(MandateError::PriceNotPaid.into());
        }
        Ok(due)
    }
}

/// # Pull
///
/// Take tokens under a policy, within every limit on the account. If the
/// policy has a price, the spender pays it to the authority here, in the same
/// instruction: there is nothing in between to trust. Callable by CPI.
///
/// > Check the window and the spender
/// > Check the amount against every limit on the source, and record it
/// > Transfer from the source, as the engine delegate
/// > Price: transfer the payment from the spender, and check what arrived
///
/// Accounts:
///
/// 1. spender:         [signer]
/// 2. policy:         [mut]
/// 3. from:            [mut]           the authority's token account
/// 4. mint:                            its mint
/// 5. to:              [mut]           where the spender sends the tokens
/// 6. engine:                          SPL delegate and event signer
/// 7. program:         [executable]    this program, for the event CPI
/// 8. token_program:   [executable]    of `from`
///
/// With a price, also:
///
/// 9. pay_from:        [mut]           the spender's token account
/// 10. pay_mint:                       the price's mint
/// 11. pay_to:         [mut]           the authority's token account the price names
/// 12. pay_program:    [executable]    token program of `pay_from`
///
/// Parameters:
/// 1. amount: u64,
///
/// Account Checks:
/// - Spender: signer; the one the terms name, checked in process
/// - Policy: writable, loaded in process
/// - From, Mint, PayTo, PayMint: the accounts the terms name, checked in process
/// - To, PayFrom, Engine, Program, token programs: no need to check since the CPIs fail otherwise
///
/// Event Data:
/// - discriminator: u8, (255u8, 1u8)
/// - policy: Pubkey,
/// - spender: Pubkey,
/// - amount: u64,
/// - paid: u64,
pub struct Pull<'a> {
    pub policy: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub legs: Legs<'a>,
    pub amount: u64,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Pull<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Pull");

        let [spender, policy, from, mint, to, engine, program, _token_program, payment @ ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !spender.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !policy.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            policy,
            program,
            legs: Legs {
                spender,
                from,
                mint,
                to,
                engine,
                payment,
            },
            amount: u64::from_le_bytes(
                data.try_into()
                    .map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
        })
    }
}

impl<'a> Pull<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let now = Clock::get()?.unix_timestamp;
        let (legs, amount) = (&self.legs, self.amount);

        let (policy, bytes) = policy(self.policy)?;
        let terms = Terms::decode(bytes)?;
        legs.check(&terms, now)?;

        // The amount must fit every limit on the source. Each limit's count is
        // written back as of now, so one timestamp serves all of them
        let mut covered = false;
        for (k, limit) in terms.limits().iter().enumerate() {
            let mut spent = policy.ledger.spent(k, limit.per, terms.not_before, now);
            if limit.from.eq(legs.from.key()) {
                if limit.mint.ne(legs.mint.key()) {
                    return Err(MandateError::InvalidTarget.into());
                }
                spent = spent.checked_add(amount).ok_or(MandateError::Overflow)?;
                if spent > limit.max {
                    return Err(MandateError::LimitExceeded.into());
                }
                covered = true;
            }
            policy.ledger.set_consumed(k, spent);
        }
        if !covered {
            return Err(MandateError::InvalidPull.into());
        }
        policy.ledger.set_rolled(now);

        let paid = legs.settle(&terms, amount, now)?;

        // Log the Pull Event
        emit(
            legs.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[
                self.policy.key(),
                legs.spender.key(),
                &amount.to_le_bytes(),
                &paid.to_le_bytes(),
            ],
        )
    }
}
