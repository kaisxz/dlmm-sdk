use anchor_lang::prelude::*;

// IMPORTANT: There is a known issue with u128 alignment in Anchor accounts
// See: https://github.com/coral-xyz/anchor/issues/3114
// This affects zero_copy accounts containing u128/i128 fields when using Rust 1.77+
// The layout/alignment changed in newer Rust versions but Solana programs use the old layout
// Workaround: Use repr(C) or a custom u128 wrapper struct if needed
#[derive(
    Debug,
    Copy,
    Clone,
    InitSpace,
    AnchorDeserialize,
    AnchorSerialize,
    PartialEq,
    bytemuck::Zeroable,
    bytemuck::Pod,
)]
#[repr(C)]
pub struct u128(pub [u8; 16]);

impl Default for u128 {
    fn default() -> Self {
        Self([0u8; 16])
    }
}

impl u128 {
    pub fn as_u128(&self) -> core::primitive::u128 {
        core::primitive::u128::from_le_bytes(self.0)
    }

    pub fn set(&mut self, val: core::primitive::u128) {
        self.0 = val.to_le_bytes();
    }
}
