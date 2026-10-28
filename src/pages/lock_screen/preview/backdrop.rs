//! The pictures under the lock, on the GPU.

use super::studio::Slot;
use crate::util::texture::Texture;

#[derive(Default)]
pub struct Backdrop {
    slots: [Option<Texture>; Slot::ALL.len()],
}

impl Backdrop {
    pub fn get(&self, slot: Slot) -> Option<&Texture> {
        self.slots[slot as usize].as_ref()
    }

    /// Puts `texture` in `slot`, handing back what was there to be freed.
    pub fn replace(&mut self, slot: Slot, texture: Option<Texture>) -> Option<Texture> {
        std::mem::replace(&mut self.slots[slot as usize], texture)
    }
}
