//! Instruction modules.
//!
//! Stage 4.1.7: `pool`, `deposit`, `withdraw`.
//! Stage 15.5:  `deposit_split`.
//!
//! We use glob re-exports because Anchor's `#[program]` macro needs to see
//! the generated `__client_accounts_*` structures for CPI. The handler
//! functions have distinct names (`handler_pool`, `handler_deposit`,
//! `handler_deposit_split`, `handler_withdraw`) to avoid ambiguity.

pub mod deposit;
pub mod deposit_split;
pub mod pool;
pub mod withdraw;

pub use deposit::*;
pub use deposit_split::*;
pub use pool::*;
pub use withdraw::*;
