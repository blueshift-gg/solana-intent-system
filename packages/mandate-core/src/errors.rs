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
    /// The account to create already holds data
    AlreadyInitialized,
    /// An account the terms or pulls reference was not supplied
    MissingAccount,

    /// The terms bytes are not a canonical encoding
    MalformedTerms,
    /// The terms break a validity rule (`Terms::validate`)
    InvalidTerms,
    /// The terms are for another cluster
    WrongCluster,
    /// The signer is not the mandate's authority, nor an executor allowed to revoke it
    InvalidAuthority,
    /// The signature does not cover the rendered terms
    InvalidSignature,
    /// A timestamp outside years 1970–9999 cannot be rendered
    Unrenderable,

    /// The mandate's window has not opened
    NotYetValid,
    /// The mandate's window has closed
    Expired,
    /// The signer is not the mandate's executor
    InvalidExecutor,
    /// The mandate was revoked or its epoch bumped
    Revoked,
    /// The mandate ran once already, or its lifetime limits are spent
    AlreadyUsed,
    /// The mandate's terms could still run: not expired, and the epoch has not moved
    NotClosable,
    /// The account is not the one that paid the rent being refunded
    InvalidPayer,

    /// An exchange's Open and its Close must be top-level instructions
    NotTopLevel,
    /// The transaction does not hold exactly one Close for this session
    InvalidSession,
    /// The session watches too many token accounts
    SessionFull,
    /// A payment's destination could count toward another mandate's
    /// requirement, so payments and sessions never share a transaction
    PaymentInSession,
    /// A pull does not come from a take of this mandate, or goes somewhere its take does not allow
    InvalidPull,
    /// A pull exceeds a take's limit
    BudgetExceeded,
    /// A target is not a token account of the stated mint and owner
    InvalidTarget,
    /// A bound or limit overflowed
    Overflow,
    /// An outcome the mandates require does not hold at Close
    OutcomeNotMet,
    /// Only the engine PDA may invoke the event instruction
    InvalidEventAuthority,
}

impl MandateError {
    /// Every error in code order, so clients can name a code: `ALL[code]`.
    /// A new variant goes here too; the test below checks the order.
    pub const ALL: [MandateError; 31] = [
        MandateError::NotSigner,
        MandateError::NotMutable,
        MandateError::InvalidAccountOwner,
        MandateError::InvalidAccountLength,
        MandateError::InvalidTag,
        MandateError::InvalidSeeds,
        MandateError::AlreadyInitialized,
        MandateError::MissingAccount,
        MandateError::MalformedTerms,
        MandateError::InvalidTerms,
        MandateError::WrongCluster,
        MandateError::InvalidAuthority,
        MandateError::InvalidSignature,
        MandateError::Unrenderable,
        MandateError::NotYetValid,
        MandateError::Expired,
        MandateError::InvalidExecutor,
        MandateError::Revoked,
        MandateError::AlreadyUsed,
        MandateError::NotClosable,
        MandateError::InvalidPayer,
        MandateError::NotTopLevel,
        MandateError::InvalidSession,
        MandateError::SessionFull,
        MandateError::PaymentInSession,
        MandateError::InvalidPull,
        MandateError::BudgetExceeded,
        MandateError::InvalidTarget,
        MandateError::Overflow,
        MandateError::OutcomeNotMet,
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
