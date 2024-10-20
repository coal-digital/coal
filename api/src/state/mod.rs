mod bus;
mod config;
mod proof;
mod proof_v2;
mod treasury;
mod tool;
mod reprocessor;
pub use bus::*;
pub use config::*;
pub use proof::*;
pub use proof_v2::*;
pub use treasury::*;
pub use tool::*;
pub use reprocessor::*;

use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum AccountDiscriminator {
    Bus = 100,
    Config = 101,
    Proof = 102,
    Treasury = 103,
    ProofV2 = 104,
    WoodConfig = 106,
    Tool = 107,
    Reprocessor = 108,
    WoodTool = 109,
}
