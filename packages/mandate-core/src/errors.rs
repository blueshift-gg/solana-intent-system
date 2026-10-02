use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MandateError {
    /// Account expected to be a signer
    NotSigner,
    /// An account this program writes directly must be writable
    NotMutable,
    /// Account expected to be owned by this program
    InvalidAccountOwner,
    /// The account data length is not the expected one
    InvalidAccountLength,
    /// The account's tag is not the expected type
    InvalidTag,
    /// The account is not the PDA for its seeds
    InvalidSeeds,
    /// The policy exists already
    AlreadyInitialized,

    /// The terms bytes are not a canonical encoding
    MalformedTerms,
    /// The terms break a validity rule (`Terms::validate`)
    InvalidTerms,
    /// The terms are for another cluster
    WrongCluster,
    /// The signer is not the mandate's authority
    InvalidAuthority,
    /// The signature does not cover the rendered terms
    InvalidSignature,
    /// A signed intent must expire, and name one source account
    InvalidIntent,
    /// The intent's nonce was used: it ran already, or was cancelled
    NonceUsed,
    /// A timestamp outside years 1970–9999 cannot be rendered
    Unrenderable,

    /// The mandate's window has not opened
    NotYetValid,
    /// The mandate's window has closed
    Expired,
    /// The signer is not the mandate's spender
    InvalidSpender,
    /// Only the authority or the spender may close a policy before it expires,
    /// and nobody may close a page of nonces before its day is over
    NotClosable,
    /// The account is not the one that paid the rent being refunded
    InvalidPayer,

    /// No limit of the policy covers the account pulled from
    InvalidPull,
    /// The pull exceeds a limit
    LimitExceeded,
    /// A token account or mint is not the one the terms name, or not the authority's
    InvalidTarget,
    /// An amount overflowed
    Overflow,
    /// The authority received less than the price requires
    PriceNotPaid,
    /// Only the engine PDA may invoke the event instruction
    InvalidEventAuthority,
}

impl MandateError {
    /// Every error in code order, so clients can name a code: `ALL[code]`.
    /// A new variant goes here too; the test below checks the order.
    pub const ALL: [MandateError; 26] = [
        MandateError::NotSigner,
        MandateError::NotMutable,
        MandateError::InvalidAccountOwner,
        MandateError::InvalidAccountLength,
        MandateError::InvalidTag,
        MandateError::InvalidSeeds,
        MandateError::AlreadyInitialized,
        MandateError::MalformedTerms,
        MandateError::InvalidTerms,
        MandateError::WrongCluster,
        MandateError::InvalidAuthority,
        MandateError::InvalidSignature,
        MandateError::InvalidIntent,
        MandateError::NonceUsed,
        MandateError::Unrenderable,
        MandateError::NotYetValid,
        MandateError::Expired,
        MandateError::InvalidSpender,
        MandateError::NotClosable,
        MandateError::InvalidPayer,
        MandateError::InvalidPull,
        MandateError::LimitExceeded,
        MandateError::InvalidTarget,
        MandateError::Overflow,
        MandateError::PriceNotPaid,
        MandateError::InvalidEventAuthority,
    ];
}

impl From<MandateError> for ProgramError {
    #[inline]
    fn from(e: MandateError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::MandateError;

    #[test]
    fn all_lists_every_error_in_code_order() {
        for (code, error) in MandateError::ALL.iter().enumerate() {
            assert_eq!(*error as usize, code);
        }
    }
}
