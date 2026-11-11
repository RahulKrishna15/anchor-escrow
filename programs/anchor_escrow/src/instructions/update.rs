use anchor_lang::prelude::*;

use crate::{constants::ESCROW_SEED, error::EscrowError, state::Escrow};

#[derive(Accounts)]
pub struct Update<'info> {
    pub maker: Signer<'info>,

    #[account(
        mut,
        has_one = maker,
        seeds = [ESCROW_SEED, maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,
}

impl<'info> Update<'info> {
    pub fn update(&mut self, new_receive: u64) -> Result<()> {
        require!(new_receive > 0, EscrowError::InvalidAmount);
        self.escrow.receive = new_receive;
        Ok(())
    }
}
