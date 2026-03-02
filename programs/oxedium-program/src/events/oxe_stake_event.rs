use anchor_lang::prelude::*;

#[event]
pub struct OxeStakeEvent {
    pub user: Pubkey,
    pub amount: u64,
}
