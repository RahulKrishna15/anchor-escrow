pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("Gqx2S5xbiYTnCASe22mVJ428qhEDcrT127eDVGbVT4V6");

#[program]
pub mod anchor_escrow {
    use super::*;

    /// Maker locks `deposit` of mint A in a vault and asks for `receive` of mint B.
    pub fn make(ctx: Context<Make>, seed: u64, deposit: u64, receive: u64) -> Result<()> {
        ctx.accounts.init_escrow(seed, receive, &ctx.bumps)?;
        ctx.accounts.deposit(deposit)
    }

    /// Taker pays `receive` of mint B to the maker and gets the vaulted mint A.
    pub fn take(ctx: Context<Take>) -> Result<()> {
        ctx.accounts.deposit()?;
        ctx.accounts.withdraw_and_close_vault()
    }

    /// Maker cancels the offer and gets their mint A back.
    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        ctx.accounts.refund_and_close_vault()
    }

    /// Maker changes the amount of mint B they want in return.
    pub fn update(ctx: Context<Update>, new_receive: u64) -> Result<()> {
        ctx.accounts.update(new_receive)
    }
}
