use anchor_lang::prelude::*;

#[event]
pub struct OxeUnstakeEvent {
    pub user: Pubkey,
    pub amount: u64,
}
