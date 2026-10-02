//! Terms: what an authority permits. Types, canonical binary codec and
//! validity rules.
//!
//! `Terms::decode` is the only way to read terms from bytes. It accepts exactly
//! the canonical encodings of valid terms, so every other module trusts them.

use crate::{constants::*, errors::MandateError, Sink};
use pinocchio::pubkey::Pubkey;

type Result<T> = core::result::Result<T, MandateError>;

const ZERO: Pubkey = [0; 32];

/// What a limit counts over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Per {
    /// The life of the mandate.
    Total,
    /// Fixed windows of `seconds`, counted from `not_before`. Nothing carries over.
    Every(u32),
    /// One pull.
    Use,
}

/// "At most `max` of `mint` may leave `from`", a token account of the
/// authority. Limits on one account stack: a pull must fit every one of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limit<'a> {
    pub from: &'a Pubkey,
    pub mint: &'a Pubkey,
    pub max: u64,
    pub per: Per,
}

/// The rate moves in a straight line to `num` between `t0` and `t1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decay {
    pub t0: i64,
    pub t1: i64,
    pub num: u64,
}

/// "For every `den` taken, at least `num` of `mint` arrives in `to`", a token
/// account of the authority. The spender pays it inside the pull.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Price<'a> {
    pub to: &'a Pubkey,
    pub mint: &'a Pubkey,
    pub num: u64,
    pub den: u64,
    pub decay: Option<Decay>,
}

#[derive(Clone, Copy, Debug)]
pub struct Terms<'a> {
    pub cluster: u8,
    pub authority: &'a Pubkey,
    /// Who may pull. `None` is anyone, which is only valid with a price.
    pub spender: Option<&'a Pubkey>,
    pub not_before: i64,
    pub not_after: Option<i64>,
    /// Tells apart mandates whose terms are otherwise identical.
    pub salt: u64,
    limits: [Limit<'a>; MAX_LIMITS],
    count: u8,
    pub price: Option<Price<'a>>,
}

impl Price<'_> {
    /// What must arrive for `amount` taken at `now`, rounded up.
    pub fn due(&self, amount: u64, now: i64) -> Result<u64> {
        let num = match self.decay {
            None => self.num,
            Some(Decay { t0, t1, num }) => {
                // Truncates toward the starting rate
                let moved = (num as i128 - self.num as i128) * (now.clamp(t0, t1) - t0) as i128;
                (self.num as i128 + moved / (t1 - t0) as i128) as u64
            }
        };
        let due = (amount as u128 * num as u128).div_ceil(self.den as u128);
        u64::try_from(due).map_err(|_| MandateError::Overflow)
    }
}

impl<'a> Terms<'a> {
    /// Terms from their parts, unchecked: `validate` or encode-then-decode them.
    pub fn new(
        authority: &'a Pubkey,
        spender: Option<&'a Pubkey>,
        window: (i64, Option<i64>),
        salt: u64,
        limits: &[Limit<'a>],
        price: Option<Price<'a>>,
    ) -> Self {
        let empty = Limit {
            from: &ZERO,
            mint: &ZERO,
            max: 0,
            per: Per::Total,
        };
        let count = limits.len().min(MAX_LIMITS);
        let mut list = [empty; MAX_LIMITS];
        list[..count].copy_from_slice(&limits[..count]);
        Terms {
            cluster: CLUSTER,
            authority,
            spender,
            not_before: window.0,
            not_after: window.1,
            salt,
            limits: list,
            // One past the maximum survives, so `validate` rejects it
            count: limits.len().min(MAX_LIMITS + 1) as u8,
            price,
        }
    }

    pub fn limits(&self) -> &[Limit<'a>] {
        &self.limits[..(self.count as usize).min(MAX_LIMITS)]
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self> {
        let mut r = Reader(bytes);
        if r.u8()? != VERSION {
            return Err(MandateError::MalformedTerms);
        }
        let cluster = r.u8()?;
        let authority = r.key()?;
        let spender = r.option(|r| r.key())?;
        let window = (r.i64()?, r.option(|r| r.i64())?);
        let salt = r.u64()?;

        let count = r.u8()? as usize;
        if count > MAX_LIMITS {
            return Err(MandateError::InvalidTerms);
        }
        let mut terms = Terms::new(authority, spender, window, salt, &[], None);
        for limit in &mut terms.limits[..count] {
            let (from, mint, max) = (r.key()?, r.key()?, r.u64()?);
            let per = match (r.u8()?, r.u32()?) {
                (0, 0) => Per::Total,
                (1, seconds) => Per::Every(seconds),
                (2, 0) => Per::Use,
                _ => return Err(MandateError::MalformedTerms),
            };
            *limit = Limit {
                from,
                mint,
                max,
                per,
            };
        }
        terms.cluster = cluster;
        terms.count = count as u8;
        terms.price = r.option(|r| {
            Ok(Price {
                to: r.key()?,
                mint: r.key()?,
                num: r.u64()?,
                den: r.u64()?,
                decay: r.option(|r| {
                    Ok(Decay {
                        t0: r.i64()?,
                        t1: r.i64()?,
                        num: r.u64()?,
                    })
                })?,
            })
        })?;
        if !r.0.is_empty() {
            return Err(MandateError::MalformedTerms);
        }
        terms.validate()?;
        Ok(terms)
    }

    /// Everything a decoder must reject beyond malformed bytes.
    pub fn validate(&self) -> Result<()> {
        if self.cluster != CLUSTER {
            return Err(MandateError::WrongCluster);
        }
        // Every timestamp is renderable, which also keeps time arithmetic from overflowing
        let renderable = |t: i64| (0..=MAX_TIME).contains(&t);
        let window = renderable(self.not_before)
            && self
                .not_after
                .is_none_or(|t| t > self.not_before && renderable(t));
        let limits = self.limits();
        let sized = (1..=MAX_LIMITS).contains(&(self.count as usize));
        let positive = limits.iter().all(|l| l.max > 0 && l.per != Per::Every(0));
        // A per-use cap alone bounds nothing across pulls: every account
        // also needs a limit that persists
        let capped = limits.iter().all(|l| {
            let mut on_account = limits.iter().filter(|m| m.from == l.from);
            on_account.any(|m| m.per != Per::Use)
        });
        let price = self.price.is_none_or(|p| {
            // One input token, so "for every `den` taken" has one meaning
            let one_source = limits.iter().all(|l| l.from == limits[0].from);
            let decay = p
                .decay
                .is_none_or(|d| renderable(d.t0) && renderable(d.t1) && d.t0 < d.t1 && d.num > 0);
            one_source && p.num > 0 && p.den > 0 && decay
        });
        // Someone must be bound: a spender, or a price the owner is paid.
        // Otherwise the mandate pays whoever finds it
        let bound = self.spender.is_some() || self.price.is_some();
        if !(window && sized && positive && capped && price && bound) {
            return Err(MandateError::InvalidTerms);
        }
        Ok(())
    }

    pub fn write(&self, w: &mut impl Sink) {
        w.put(&[VERSION, self.cluster]);
        w.put(self.authority);
        option(w, self.spender, |w, key| w.put(key));
        w.put(&self.not_before.to_le_bytes());
        option(w, self.not_after, |w, t| w.put(&t.to_le_bytes()));
        w.put(&self.salt.to_le_bytes());
        w.put(&[self.count]);
        for limit in self.limits() {
            w.put(limit.from);
            w.put(limit.mint);
            w.put(&limit.max.to_le_bytes());
            let (tag, seconds) = match limit.per {
                Per::Total => (0, 0),
                Per::Every(seconds) => (1, seconds),
                Per::Use => (2, 0),
            };
            w.put(&[tag]);
            w.put(&seconds.to_le_bytes());
        }
        option(w, self.price, |w, p| {
            w.put(p.to);
            w.put(p.mint);
            w.put(&p.num.to_le_bytes());
            w.put(&p.den.to_le_bytes());
            option(w, p.decay, |w, d| {
                w.put(&d.t0.to_le_bytes());
                w.put(&d.t1.to_le_bytes());
                w.put(&d.num.to_le_bytes());
            });
        });
    }
}

fn option<W: Sink, T>(w: &mut W, value: Option<T>, write: impl FnOnce(&mut W, T)) {
    match value {
        None => w.put(&[0]),
        Some(value) => {
            w.put(&[1]);
            write(w, value);
        }
    }
}

/// A cursor over canonical bytes. Every read fails on truncation.
struct Reader<'a>(&'a [u8]);

macro_rules! read_le {
    ($($name:ident: $t:ty),*) => {$(
        fn $name(&mut self) -> Result<$t> {
            Ok(<$t>::from_le_bytes(*self.take()?))
        }
    )*};
}

impl<'a> Reader<'a> {
    fn take<const N: usize>(&mut self) -> Result<&'a [u8; N]> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(MandateError::MalformedTerms)?;
        self.0 = rest;
        Ok(head)
    }

    read_le!(u8: u8, u32: u32, u64: u64, i64: i64);

    fn key(&mut self) -> Result<&'a Pubkey> {
        self.take()
    }

    fn option<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<Option<T>> {
        match self.u8()? {
            0 => Ok(None),
            1 => read(self).map(Some),
            _ => Err(MandateError::MalformedTerms),
        }
    }
}
