//! Canonical text: what the authority sees and signs.
//!
//! Deterministic, printable ASCII only, worst case first. The program renders
//! straight into the signature hasher; clients render into a buffer. Both run
//! this code, so what is shown is what is verified.

use crate::{
    constants::{CLUSTER, MAX_TIME},
    errors::MandateError,
    terms::{Bound, Refill, Require, Terms},
    Sink, ID,
};
use pinocchio::pubkey::Pubkey;

type Result<T> = core::result::Result<T, MandateError>;

/// Offchain Message v1 signing domain.
pub const DOMAIN: &[u8; 16] = b"\xffsolana offchain";
const CLUSTERS: [&str; 4] = ["mainnet", "devnet", "testnet", "localnet"];

/// The Offchain Message v1 preamble: domain, version 1, one signer.
pub fn envelope(authority: &Pubkey, out: &mut impl Sink) {
    out.put(DOMAIN);
    out.put(&[1, 1]);
    out.put(authority);
}

/// Render `terms`. `decimals` resolves a mint's on-chain decimals.
pub fn render(
    terms: &Terms,
    decimals: impl Fn(&Pubkey) -> Result<u8>,
    out: &mut impl Sink,
) -> Result<()> {
    let mut t = Text { out, decimals };

    t.s("Solana Mandate v1\ncluster: ");
    t.s(CLUSTERS[CLUSTER as usize]);
    t.s("\nengine: ");
    t.key(&ID);
    t.s("\nauthority: ");
    t.key(terms.authority);

    let mut index = 0;
    let mut anywhere = false;
    for take in terms.takes.iter() {
        t.line(&mut index);
        t.s(if take.to.is_empty() {
            "MAY TAKE: at most "
        } else {
            "MAY PAY: at most "
        });
        t.amount(take.max as u128, (t.decimals)(take.mint)?);
        t.s(" of mint ");
        t.key(take.mint);
        t.s(" from ");
        t.key(take.from);
        for (i, to) in take.to.iter().enumerate() {
            t.s(if i == 0 { " to " } else { " or " });
            t.key(to);
        }
        anywhere |= take.to.is_empty();
        match take.refill {
            Refill::Never => t.s(" in total"),
            Refill::Over { period } => {
                t.s(", refilling over ");
                t.duration(period as u64);
            }
            Refill::EachUse => t.s(" per use"),
        }
    }
    for require in terms.requires.iter() {
        t.line(&mut index);
        t.s("REQUIRES: ");
        t.outcome(&require, terms)?;
    }
    if anywhere && terms.requires.is_empty() {
        t.s("\nREQUIRES: nothing - no guarantee on what you get back");
    }

    t.s("\nEXECUTOR: ");
    match terms.executor {
        None => t.s("anyone"),
        Some(key) => t.key(key),
    }
    t.s("\nVALID: from ");
    t.time(terms.not_before)?;
    t.s(" until ");
    match terms.not_after {
        None => t.s("revoked"),
        Some(end) => t.time(end)?,
    }
    t.s("\nREPLAY: ");
    t.s(if terms.once {
        "once"
    } else {
        "any number of times"
    });
    t.s("\nEPOCH: ");
    t.uint(terms.epoch as u128);
    Ok(())
}

struct Text<'o, O, D> {
    out: &'o mut O,
    decimals: D,
}

impl<O: Sink, D: Fn(&Pubkey) -> Result<u8>> Text<'_, O, D> {
    /// `\n[i] `, the prefix of every take and requirement.
    fn line(&mut self, index: &mut u8) {
        self.s("\n[");
        self.uint(*index as u128);
        self.s("] ");
        *index += 1;
    }

    /// `account gains at least value` for every requirement.
    fn outcome(&mut self, a: &Require, terms: &Terms) -> Result<()> {
        let decimals = (self.decimals)(a.mint)?;
        self.key(a.target);
        self.s(" (mint ");
        self.key(a.mint);
        self.s(", owner ");
        self.key(a.owner);
        self.s(") gains at least ");

        match a.bound {
            Bound::Const(v) => self.amount(v as u128, decimals),
            Bound::Linear { t0, v0, t1, v1 } => {
                self.amount(v0 as u128, decimals);
                self.s(" at ");
                self.time(t0)?;
                self.s(" moving linearly to ");
                self.amount(v1 as u128, decimals);
                self.s(" at ");
                self.time(t1)?;
            }
            Bound::Ratio { of, num, den } => {
                self.amount(num as u128, decimals);
                self.s(" for every ");
                let source = terms
                    .takes
                    .get(of as usize)
                    .ok_or(MandateError::InvalidTerms)?;
                self.amount(den as u128, (self.decimals)(source.mint)?);
                self.s(" taken by [");
                self.uint(of as u128);
                self.s("]");
            }
        }
        Ok(())
    }

    fn s(&mut self, s: &str) {
        self.out.put(s.as_bytes());
    }

    fn key(&mut self, key: &Pubkey) {
        let mut out = [0; five8::BASE58_ENCODED_32_MAX_LEN];
        let len = five8::encode_32(key, &mut out) as usize;
        self.out.put(&out[..len]);
    }

    fn uint(&mut self, v: u128) {
        let (digits, len) = digits(v);
        self.out.put(&digits[digits.len() - len..]);
    }

    /// `raw` with exactly `decimals` fractional digits.
    fn amount(&mut self, raw: u128, decimals: u8) {
        let (digits, len) = digits(raw);
        let digits = &digits[digits.len() - len..];
        let d = decimals as usize;
        if d == 0 {
            self.out.put(digits);
        } else if len <= d {
            self.s("0.");
            (len..d).for_each(|_| self.s("0"));
            self.out.put(digits);
        } else {
            self.out.put(&digits[..len - d]);
            self.s(".");
            self.out.put(&digits[len - d..]);
        }
    }

    /// `YYYY-MM-DDTHH:MM:SSZ`, years 1970–9999 (civil-from-days, H. Hinnant).
    fn time(&mut self, t: i64) -> Result<()> {
        if !(0..=MAX_TIME).contains(&t) {
            return Err(MandateError::Unrenderable);
        }
        let (days, secs) = (t / 86_400, t % 86_400);
        let z = days + 719_468;
        let (era, doe) = (z / 146_097, z % 146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + (month <= 2) as i64;
        for (v, width, sep) in [
            (year, 4, "-"),
            (month, 2, "-"),
            (day, 2, "T"),
            (secs / 3_600, 2, ":"),
            (secs / 60 % 60, 2, ":"),
            (secs % 60, 2, "Z"),
        ] {
            let (digits, len) = digits(v as u128);
            (len..width).for_each(|_| self.s("0"));
            self.out.put(&digits[digits.len() - len..]);
            self.s(sep);
        }
        Ok(())
    }

    /// `1d2h3m4s`, zero components omitted, `0s` for zero.
    fn duration(&mut self, secs: u64) {
        if secs == 0 {
            return self.s("0s");
        }
        let parts = [
            (secs / 86_400, "d"),
            (secs / 3_600 % 24, "h"),
            (secs / 60 % 60, "m"),
            (secs % 60, "s"),
        ];
        for (n, unit) in parts.into_iter().filter(|(n, _)| *n > 0) {
            self.uint(n as u128);
            self.s(unit);
        }
    }
}

/// Decimal digits of `v`, right-aligned, and how many there are. Values that
/// fit in 64 bits skip 128-bit division, which sBPF emulates slowly.
fn digits(mut v: u128) -> ([u8; 39], usize) {
    let mut out = [b'0'; 39];
    let mut len = 0;
    while v > u64::MAX as u128 {
        out[38 - len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
    }
    let mut v = v as u64;
    loop {
        out[38 - len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
        if v == 0 {
            return (out, len);
        }
    }
}
