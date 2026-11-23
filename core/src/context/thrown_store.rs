//! Generational arena for storing JS thrown/rejected values across async boundaries.

use crate::JSValueImpl;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ThrownValueHandle {
    /// The capturing context's serial, never reused, unlike its address.
    pub(crate) serial: u64,
    pub(crate) id: u32,
    pub(crate) generation: u32,
}

#[derive(Debug)]
pub(crate) struct ThrownValueStore<V: JSValueImpl> {
    slots: Vec<ThrownSlot<V>>,
    free: Vec<usize>,
}

#[derive(Debug)]
struct ThrownSlot<V: JSValueImpl> {
    generation: u32,
    value: Option<V>,
}

impl<V: JSValueImpl> ThrownValueStore<V> {
    pub(crate) fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, serial: u64, value: V) -> ThrownValueHandle {
        if let Some(id) = self.free.pop() {
            let slot = &mut self.slots[id];
            slot.generation = slot.generation.wrapping_add(1).max(1);
            slot.value = Some(value);
            return ThrownValueHandle {
                serial,
                id: id as u32,
                generation: slot.generation,
            };
        }

        let id = self.slots.len();
        self.slots.push(ThrownSlot {
            generation: 1,
            value: Some(value),
        });
        ThrownValueHandle {
            serial,
            id: id as u32,
            generation: 1,
        }
    }

    pub(crate) fn get(&self, serial: u64, handle: ThrownValueHandle) -> Option<V> {
        if handle.serial != serial {
            return None;
        }

        let id = handle.id as usize;
        let slot = self.slots.get(id)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.value.clone()
    }

    pub(crate) fn take(&mut self, serial: u64, handle: ThrownValueHandle) -> Option<V> {
        if handle.serial != serial {
            return None;
        }

        let id = handle.id as usize;
        let slot = self.slots.get_mut(id)?;
        if slot.generation != handle.generation {
            return None;
        }

        let value = slot.value.take();
        if value.is_some() {
            self.free.push(id);
        }
        value
    }
}
