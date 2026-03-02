use anchor_lang::prelude::*;

#[event]
pub struct OxeClaimEvent {
    pub user: Pubkey,
    pub vault: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
}
