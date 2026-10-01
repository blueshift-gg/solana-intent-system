//! Terms: types, canonical binary codec and validity rules.
//!
//! `Terms::decode` is the only way to read terms from bytes. It accepts exactly
//! the canonical encodings of valid terms, so every other module trusts them.

use crate::{constants::*, errors::MandateError, Sink};
use pinocchio::pubkey::Pubkey;

type Result<T> = core::result::Result<T, MandateError>;

/// How a take's limit comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refill {
    /// A lifetime total.
    Never,
    /// Linearly: all of it over `period` seconds.
    Over { period: u32 },
    /// In full for every execution: a per-use cap, on top of a limit that persists.
    EachUse,
}

/// "At most `max` of `mint` may leave `from`", a token account of the
/// authority: the only thing that permits a pull. Takes on one account stack:
/// a pull must fit every one of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Take<'a> {
    pub from: &'a Pubkey,
    pub mint: &'a Pubkey,
    pub max: u64,
    pub refill: Refill,
    /// Where a pull may go. Empty: wherever the executor sends it.
    pub to: &'a [Pubkey],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    Const(u64),
    /// Moves from `v0` at `t0` to `v1` at `t1`, clamped outside; truncates toward `v0`.
    Linear {
        t0: i64,
        v0: u64,
        t1: i64,
        v1: u64,
    },
    /// At least `num / den` of what take `of` gave up, rounded up.
    Ratio {
        of: u8,
        num: u64,
        den: u64,
    },
}

/// Token account `target`, of `mint` and owned by `owner`, must gain at least
/// `bound` between `Open` and `Close`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Require<'a> {
    pub target: &'a Pubkey,
    pub mint: &'a Pubkey,
    pub owner: &'a Pubkey,
    pub bound: Bound,
}

#[derive(Clone, Copy, Debug)]
pub struct Terms<'a> {
    pub cluster: u8,
    pub authority: &'a Pubkey,
    pub executor: Option<&'a Pubkey>,
    pub not_before: i64,
    pub not_after: Option<i64>,
    /// One execution only, whatever it takes.
    pub once: bool,
    pub epoch: u32,
    pub takes: Seq<'a, Take<'a>>,
    pub requires: Seq<'a, Require<'a>>,
}

/// An element of a `Seq`.
pub trait Item<'a>: Copy {
    fn read(r: &mut Reader<'a>) -> Result<Self>;
    fn write(&self, w: &mut impl Sink);
}

/// Decoded terms keep their lists encoded and decode them on iteration, so
/// the program never copies them; builders pass a slice.
#[derive(Clone, Copy, Debug)]
pub enum Seq<'a, T> {
    Encoded { bytes: &'a [u8], len: u8 },
    List(&'a [T]),
}

impl<'a, T: Item<'a>> Seq<'a, T> {
    pub fn len(&self) -> usize {
        match self {
            Seq::Encoded { len, .. } => *len as usize,
            Seq::List(list) => list.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> SeqIter<'a, T> {
        match *self {
            Seq::Encoded { bytes, len } => SeqIter::Encoded(Reader(bytes), len),
            Seq::List(list) => SeqIter::List(list.iter()),
        }
    }

    pub fn get(&self, index: usize) -> Option<T> {
        self.iter().nth(index)
    }

    /// Read a length-prefixed list, decoding every element once.
    fn read(r: &mut Reader<'a>) -> Result<Self> {
        let len = r.u8()?;
        let start = r.0;
        for _ in 0..len {
            T::read(r)?;
        }
        let bytes = &start[..start.len() - r.0.len()];
        Ok(Seq::Encoded { bytes, len })
    }

    fn write(&self, w: &mut impl Sink) {
        w.put(&[self.len() as u8]);
        match self {
            Seq::Encoded { bytes, .. } => w.put(bytes),
            Seq::List(list) => list.iter().for_each(|x| x.write(w)),
        }
    }
}

pub enum SeqIter<'a, T> {
    Encoded(Reader<'a>, u8),
    List(core::slice::Iter<'a, T>),
}

impl<'a, T: Item<'a>> Iterator for SeqIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match self {
            SeqIter::Encoded(reader, left) => {
                *left = left.checked_sub(1)?;
                // Encoded lists were decoded once by `Terms::decode`.
                T::read(reader).ok()
            }
            SeqIter::List(list) => list.next().copied(),
        }
    }
}

impl<'a> Item<'a> for Take<'a> {
    fn read(r: &mut Reader<'a>) -> Result<Self> {
        let (from, mint, max) = (r.key()?, r.key()?, r.u64()?);
        let refill = match r.u8()? {
            0 => Refill::Never,
            1 => Refill::Over { period: r.u32()? },
            2 => Refill::EachUse,
            _ => return Err(MandateError::MalformedTerms),
        };
        let to = r.keys()?;
        Ok(Self {
            from,
            mint,
            max,
            refill,
            to,
        })
    }

    fn write(&self, w: &mut impl Sink) {
        w.put(self.from);
        w.put(self.mint);
        w.put(&self.max.to_le_bytes());
        match self.refill {
            Refill::Never => w.put(&[0]),
            Refill::Over { period } => {
                w.put(&[1]);
                w.put(&period.to_le_bytes());
            }
            Refill::EachUse => w.put(&[2]),
        }
        w.put(&[self.to.len() as u8]);
        self.to.iter().for_each(|key| w.put(key));
    }
}

impl<'a> Item<'a> for Require<'a> {
    fn read(r: &mut Reader<'a>) -> Result<Self> {
        let (target, mint, owner) = (r.key()?, r.key()?, r.key()?);
        let bound = match r.u8()? {
            0 => Bound::Const(r.u64()?),
            1 => Bound::Linear {
                t0: r.i64()?,
                v0: r.u64()?,
                t1: r.i64()?,
                v1: r.u64()?,
            },
            2 => Bound::Ratio {
                of: r.u8()?,
                num: r.u64()?,
                den: r.u64()?,
            },
            _ => return Err(MandateError::MalformedTerms),
        };
        Ok(Self {
            target,
            mint,
            owner,
            bound,
        })
    }

    fn write(&self, w: &mut impl Sink) {
        w.put(self.target);
        w.put(self.mint);
        w.put(self.owner);
        match self.bound {
            Bound::Const(c) => {
                w.put(&[0]);
                w.put(&c.to_le_bytes());
            }
            Bound::Linear { t0, v0, t1, v1 } => {
                w.put(&[1]);
                w.put(&t0.to_le_bytes());
                w.put(&v0.to_le_bytes());
                w.put(&t1.to_le_bytes());
                w.put(&v1.to_le_bytes());
            }
            Bound::Ratio { of, num, den } => {
                w.put(&[2, of]);
                w.put(&num.to_le_bytes());
                w.put(&den.to_le_bytes());
            }
        }
    }
}

impl Bound {
    /// The bound's value at `now`; `taken` is what each take gave up.
    pub fn at(&self, now: i64, taken: &[u64]) -> Result<u64> {
        let overflow = MandateError::Overflow;
        Ok(match *self {
            Bound::Const(c) => c,
            Bound::Linear { t0, v0, t1, v1 } => {
                let t = now.clamp(t0, t1);
                let moved = (v1 as i128 - v0 as i128) * (t - t0) as i128 / (t1 - t0) as i128;
                (v0 as i128 + moved) as u64
            }
            Bound::Ratio { of, num, den } => {
                let taken = *taken.get(of as usize).ok_or(overflow)?;
                let scaled = num as u128 * taken as u128;
                u64::try_from(scaled.div_ceil(den as u128)).map_err(|_| overflow)?
            }
        })
    }

    /// Positive, renderable, and a ratio of a take that exists.
    fn check(&self, takes: usize) -> bool {
        match *self {
            Bound::Const(c) => c > 0,
            Bound::Linear { t0, t1, .. } => {
                (0..=MAX_TIME).contains(&t0) && t0 < t1 && t1 <= MAX_TIME
            }
            Bound::Ratio { of, den, .. } => (of as usize) < takes && den > 0,
        }
    }
}

impl<'a> Terms<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self> {
        let mut r = Reader(bytes);
        if r.u8()? != VERSION {
            return Err(MandateError::MalformedTerms);
        }
        let terms = Terms {
            cluster: r.u8()?,
            authority: r.key()?,
            executor: r.option(|r| r.key())?,
            not_before: r.i64()?,
            not_after: r.option(|r| r.i64())?,
            once: r.bool()?,
            epoch: r.u32()?,
            takes: Seq::read(&mut r)?,
            requires: Seq::read(&mut r)?,
        };
        if !r.0.is_empty() {
            return Err(MandateError::MalformedTerms);
        }
        terms.validate()?;
        Ok(terms)
    }

    /// The validity rules: everything a decoder must reject beyond malformed bytes.
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
        let lines = self.takes.len() + self.requires.len();
        let takes = self
            .takes
            .iter()
            .all(|t| t.max > 0 && t.refill != Refill::Over { period: 0 });
        // A per-use cap alone bounds nothing across executions: unless the
        // mandate runs once, every account also needs a limit that persists
        let capped = self.once
            || self.takes.iter().all(|t| {
                let mut on_account = self.takes.iter().filter(|u| u.from == t.from);
                on_account.any(|u| u.refill != Refill::EachUse)
            });
        let requires = self
            .requires
            .iter()
            .all(|x| x.bound.check(self.takes.len()));
        // A destination is a payment; a requirement is an exchange. A mandate
        // is one or the other, so a payment never has to join a session
        let mixed = !self.requires.is_empty() && self.takes.iter().any(|t| !t.to.is_empty());
        if !window
            || self.takes.is_empty()
            || lines > MAX_ASSERTS
            || !takes
            || !capped
            || !requires
            || mixed
        {
            return Err(MandateError::InvalidTerms);
        }
        Ok(())
    }

    pub fn write(&self, w: &mut impl Sink) {
        w.put(&[VERSION, self.cluster]);
        w.put(self.authority);
        match self.executor {
            None => w.put(&[0]),
            Some(key) => {
                w.put(&[1]);
                w.put(key);
            }
        }
        w.put(&self.not_before.to_le_bytes());
        match self.not_after {
            None => w.put(&[0]),
            Some(t) => {
                w.put(&[1]);
                w.put(&t.to_le_bytes());
            }
        }
        w.put(&[self.once as u8]);
        w.put(&self.epoch.to_le_bytes());
        self.takes.write(w);
        self.requires.write(w);
    }
}

/// A cursor over canonical bytes. Every read fails on truncation.
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a>(pub &'a [u8]);

macro_rules! read_le {
    ($($name:ident: $t:ty),*) => {$(
        pub fn $name(&mut self) -> Result<$t> {
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

    pub fn key(&mut self) -> Result<&'a Pubkey> {
        self.take()
    }

    /// A length-prefixed list of keys, viewed in place.
    fn keys(&mut self) -> Result<&'a [Pubkey]> {
        let len = self.u8()? as usize * 32;
        let (head, rest) = self
            .0
            .split_at_checked(len)
            .ok_or(MandateError::MalformedTerms)?;
        self.0 = rest;
        Ok(head.as_chunks().0)
    }

    fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(MandateError::MalformedTerms),
        }
    }

    fn option<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<Option<T>> {
        match self.u8()? {
            0 => Ok(None),
            1 => read(self).map(Some),
            _ => Err(MandateError::MalformedTerms),
        }
    }
}
