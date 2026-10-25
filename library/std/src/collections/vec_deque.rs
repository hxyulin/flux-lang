//! A bounded queue API with safe ring storage for compiler worklists.
use crate::alloc_crate::vec::Vec;

pub struct VecDeque<T> { slots: Vec<Option<T>>, head: usize, len: usize }
impl<T> VecDeque<T> {
    pub const fn new() -> Self { Self { slots: Vec::new(), head: 0, len: 0 } }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    fn grow(&mut self) {
        let capacity = if self.slots.is_empty() { 4 } else { self.slots.len().checked_mul(2).expect("capacity overflow") };
        let mut slots = Vec::with_capacity(capacity);
        slots.resize_with(capacity, || None);
        for index in 0..self.len {
            let old = (self.head + index) % self.slots.len();
            slots[index] = self.slots[old].take();
        }
        self.slots = slots;
        self.head = 0;
    }
    pub fn push_back(&mut self, value: T) {
        if self.len == self.slots.len() { self.grow(); }
        let index = (self.head + self.len) % self.slots.len();
        self.slots[index] = Some(value); self.len += 1;
    }
    pub fn push_front(&mut self, value: T) {
        if self.len == self.slots.len() { self.grow(); }
        self.head = if self.head == 0 { self.slots.len() - 1 } else { self.head - 1 };
        self.slots[self.head] = Some(value); self.len += 1;
    }
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() { return None; }
        let index = self.head;
        self.head = (self.head + 1) % self.slots.len(); self.len -= 1;
        self.slots[index].take()
    }
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() { return None; }
        self.len -= 1;
        let index = (self.head + self.len) % self.slots.len();
        self.slots[index].take()
    }
    pub fn front(&self) -> Option<&T> {
        if self.is_empty() { None } else { self.slots[self.head].as_ref() }
    }
    pub fn back(&self) -> Option<&T> {
        if self.is_empty() { None } else { self.slots[(self.head + self.len - 1) % self.slots.len()].as_ref() }
    }
    pub fn clear(&mut self) { while let Some(value) = self.pop_front() { drop(value); } }
}
impl<T> Default for VecDeque<T> { fn default() -> Self { Self::new() } }
impl<T: Clone> Clone for VecDeque<T> {
    fn clone(&self) -> Self {
        let mut result = Self::new();
        for index in 0..self.len {
            let value = self.slots[(self.head + index) % self.slots.len()].as_ref().unwrap();
            result.push_back(value.clone());
        }
        result
    }
}
impl<T: core::fmt::Debug> core::fmt::Debug for VecDeque<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut list = f.debug_list();
        for index in 0..self.len {
            list.entry(self.slots[(self.head + index) % self.slots.len()].as_ref().unwrap());
        }
        list.finish()
    }
}
