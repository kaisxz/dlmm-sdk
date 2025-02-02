use anchor_lang::prelude::*;

#[derive(Debug, Copy, Clone, InitSpace, AnchorDeserialize, AnchorSerialize, PartialEq, bytemuck::Zeroable, bytemuck::Pod)]
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
