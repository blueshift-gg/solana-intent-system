// Host builds (unit tests, clippy) compile the handlers without an entrypoint
// that calls them; silence the resulting dead-code noise there only.
#![cfg_attr(not(target_os = "solana"), allow(dead_code, unused_imports))]

use pinocchio::{
    account_info::AccountInfo, default_panic_handler, no_allocator, program_entrypoint,
    program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};

// The program never allocates; `no_allocator!` turns an accidental heap use
// into a hard failure.
program_entrypoint!(process_instruction);
no_allocator!();
default_panic_handler!();

pub mod close;
pub mod create;
pub mod events;
pub mod helpers;
pub mod pull;
pub mod state;

pub use close::Close;
pub use create::Create;
pub use mandate_core::{constants, errors, ID};
pub use pull::Pull;

fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        // Pull runs on every payment, so it comes first
        Some((Pull::DISCRIMINATOR, data)) => Pull::try_from((data, accounts))?.process(),
        Some((Create::DISCRIMINATOR, data)) => Create::try_from((data, accounts))?.process(),
        Some((Close::DISCRIMINATOR, _)) => Close::try_from(accounts)?.process(),

        // Self-CPI EmitEvent - Discriminator 255
        Some((&constants::EVENT_DISCRIMINATOR, _)) => events::emit_event(accounts),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
