//! mandate-core for JavaScript. Terms cross the boundary as JSON; bytes and
//! text come from the same code the program runs, so no JavaScript
//! reimplements the codec, the validity rules or the canonical text.
//!
//! Integers that can exceed 2^53 (amounts, bounds, the epoch, the salt) are strings.

use mandate_core::constants::CLUSTER;
use mandate_core::render::render;
use mandate_core::terms::{Bound, Refill, Require, Seq, Take, Terms};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TermsJson {
    authority: String,
    executor: Option<String>,
    not_before: i64,
    not_after: Option<i64>,
    once: bool,
    epoch: String,
    salt: String,
    takes: Vec<TakeJson>,
    requires: Vec<RequireJson>,
}

#[derive(Serialize, Deserialize)]
struct TakeJson {
    from: String,
    mint: String,
    max: String,
    refill: RefillJson,
    to: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum RefillJson {
    Never,
    Over { period: u32 },
    EachUse,
}

#[derive(Serialize, Deserialize)]
struct RequireJson {
    target: String,
    mint: String,
    owner: String,
    bound: BoundJson,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum BoundJson {
    Const(String),
    Linear {
        t0: i64,
        v0: String,
        t1: i64,
        v1: String,
    },
    Ratio {
        of: u8,
        num: String,
        den: String,
    },
}

/// Canonical bytes for the terms, after the validity rules (`Terms::validate`).
#[wasm_bindgen(js_name = encodeTerms)]
pub fn encode_terms(json: &str) -> Result<Vec<u8>, JsError> {
    let j: TermsJson = serde_json::from_str(json)?;
    let authority = key(&j.authority)?;
    let executor = j.executor.as_deref().map(key).transpose()?;

    // Own every key and number first; the core's terms borrow them.
    let owned_takes = j
        .takes
        .iter()
        .map(|t| {
            let to = t.to.iter().map(|k| key(k)).collect::<Result<Vec<_>, _>>()?;
            Ok((key(&t.from)?, key(&t.mint)?, t.max.parse()?, to))
        })
        .collect::<Result<Vec<([u8; 32], [u8; 32], u64, Vec<[u8; 32]>)>, JsError>>()?;
    let takes: Vec<Take> = owned_takes
        .iter()
        .zip(&j.takes)
        .map(|((from, mint, max, to), t)| Take {
            from,
            mint,
            max: *max,
            refill: match t.refill {
                RefillJson::Never => Refill::Never,
                RefillJson::Over { period } => Refill::Over { period },
                RefillJson::EachUse => Refill::EachUse,
            },
            to,
        })
        .collect();
    let owned_requires = j
        .requires
        .iter()
        .map(|a| {
            let bound = match &a.bound {
                BoundJson::Const(v) => Bound::Const(v.parse()?),
                BoundJson::Linear { t0, v0, t1, v1 } => Bound::Linear {
                    t0: *t0,
                    v0: v0.parse()?,
                    t1: *t1,
                    v1: v1.parse()?,
                },
                BoundJson::Ratio { of, num, den } => Bound::Ratio {
                    of: *of,
                    num: num.parse()?,
                    den: den.parse()?,
                },
            };
            Ok((key(&a.target)?, key(&a.mint)?, key(&a.owner)?, bound))
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let requires: Vec<Require> = owned_requires
        .iter()
        .map(|(target, mint, owner, bound)| Require {
            target,
            mint,
            owner,
            bound: *bound,
        })
        .collect();

    let terms = Terms {
        cluster: CLUSTER,
        authority: &authority,
        executor: executor.as_ref(),
        not_before: j.not_before,
        not_after: j.not_after,
        once: j.once,
        epoch: j.epoch.parse()?,
        salt: j.salt.parse()?,
        takes: Seq::List(&takes),
        requires: Seq::List(&requires),
    };
    terms.validate().map_err(error)?;
    let mut bytes = Vec::new();
    terms.write(&mut bytes);
    Ok(bytes)
}

/// The terms behind canonical bytes, as JSON. Fails on anything non-canonical.
#[wasm_bindgen(js_name = decodeTerms)]
pub fn decode_terms(bytes: &[u8]) -> Result<String, JsError> {
    let t = Terms::decode(bytes).map_err(error)?;
    let takes = t
        .takes
        .iter()
        .map(|a| TakeJson {
            from: b58(a.from),
            mint: b58(a.mint),
            max: a.max.to_string(),
            refill: match a.refill {
                Refill::Never => RefillJson::Never,
                Refill::Over { period } => RefillJson::Over { period },
                Refill::EachUse => RefillJson::EachUse,
            },
            to: a.to.iter().map(b58).collect(),
        })
        .collect();
    let requires = t
        .requires
        .iter()
        .map(|a| RequireJson {
            target: b58(a.target),
            mint: b58(a.mint),
            owner: b58(a.owner),
            bound: match a.bound {
                Bound::Const(v) => BoundJson::Const(v.to_string()),
                Bound::Linear { t0, v0, t1, v1 } => BoundJson::Linear {
                    t0,
                    v0: v0.to_string(),
                    t1,
                    v1: v1.to_string(),
                },
                Bound::Ratio { of, num, den } => BoundJson::Ratio {
                    of,
                    num: num.to_string(),
                    den: den.to_string(),
                },
            },
        })
        .collect();
    let json = TermsJson {
        authority: b58(t.authority),
        executor: t.executor.map(b58),
        not_before: t.not_before,
        not_after: t.not_after,
        once: t.once,
        epoch: t.epoch.to_string(),
        salt: t.salt.to_string(),
        takes,
        requires,
    };
    Ok(serde_json::to_string(&json)?)
}

/// The canonical text a wallet shows and signs. `decimals` maps each mint to its decimals.
#[wasm_bindgen(js_name = renderText)]
pub fn render_text(bytes: &[u8], decimals: &str) -> Result<String, JsError> {
    let decimals: HashMap<String, u8> = serde_json::from_str(decimals)?;
    let terms = Terms::decode(bytes).map_err(error)?;
    let mut out = Vec::new();
    render(
        &terms,
        |mint| {
            decimals
                .get(&b58(mint))
                .copied()
                .ok_or(mandate_core::errors::MandateError::MissingAccount)
        },
        &mut out,
    )
    .map_err(error)?;
    Ok(String::from_utf8(out).expect("canonical text is ASCII"))
}

/// The name of a Mandate program error code, as a client sees it in a failed transaction.
#[wasm_bindgen(js_name = errorName)]
pub fn error_name(code: u32) -> Option<String> {
    mandate_core::errors::MandateError::ALL
        .get(code as usize)
        .map(|e| format!("{e:?}"))
}

fn key(s: &str) -> Result<[u8; 32], JsError> {
    let mut out = [0; 32];
    five8::decode_32(s, &mut out).map_err(|_| JsError::new(&format!("not an address: {s}")))?;
    Ok(out)
}

fn b58(key: &[u8; 32]) -> String {
    let mut out = [0; five8::BASE58_ENCODED_32_MAX_LEN];
    let len = five8::encode_32(key, &mut out) as usize;
    String::from_utf8_lossy(&out[..len]).into_owned()
}

fn error(e: mandate_core::errors::MandateError) -> JsError {
    JsError::new(&format!("{e:?}"))
}
