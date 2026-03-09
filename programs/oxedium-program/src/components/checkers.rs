use anchor_lang::prelude::*;
use crate::{states::Admin, utils::OxediumError};

/// Checks if the given signer is the admin of the treasury.
/// Returns `InvalidAdmin` error if not.
pub fn check_admin(treasury_pda: &Admin, signer: &Signer) -> Result<()> {
    if signer.key() != treasury_pda.pubkey {
        return Err(OxediumError::InvalidAdmin.into());
    }
    
    Ok(())
}