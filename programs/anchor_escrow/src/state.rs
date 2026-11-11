use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Escrow {
    /// Lets one maker run many escrows at once (part of the PDA seeds).
    pub seed: u64,
    pub maker: Pubkey,
    /// Token the maker deposits.
    pub mint_a: Pubkey,
    /// Token the maker wants in return.
    pub mint_b: Pubkey,
    /// Amount of mint B the maker wants.
    pub receive: u64,
    pub bump: u8,
}
