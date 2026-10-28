//! A bounded queue API with safe ring storage for compiler worklists.
use crate::alloc_crate::vec::Vec;

pub struct VecDeque<T> { slots: Vec<Option<T>>, head: usize, len: usize }
impl<T> VecDeque<T> {
    pub const fn new() -> Self { Self { slots: Vec::new(), head: 0, len: 0 } }
    pub fn with_capacity(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        slots.resize_with(capacity, || None);
        Self { slots, head: 0, len: 0 }
    }
    pub fn capacity(&self) -> usize { self.slots.len() }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    /// Reserves room for at least `additional` more elements.
    /// Panics if the requested capacity overflows.
    pub fn reserve(&mut self, additional: usize) {
        let required = self.len.checked_add(additional).expect("capacity overflow");
        if required <= self.capacity() { return; }
        let capacity = required.max(self.capacity().saturating_mul(2)).max(4);
        let mut slots = Vec::with_capacity(capacity);
        slots.resize_with(capacity, || None);
        for index in 0..self.len {
            let old = (self.head + index) % self.slots.len();
            slots[index] = self.slots[old].take();
        }
        self.slots = slots;
        self.head = 0;
    }
    fn grow(&mut self) { self.reserve(1); }
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
    pub fn iter(&self) -> Iter<'_, T> { Iter { slots: &self.slots, front: self.head, len: self.len } }
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

impl<T> Extend<T> for VecDeque<T> {
    fn extend<I: IntoIterator<Item=T>>(&mut self, values:I) {
        for value in values { self.push_back(value); }
    }
}
impl<T> FromIterator<T> for VecDeque<T> {
    fn from_iter<I: IntoIterator<Item=T>>(values:I) -> Self {
        let mut result=Self::new();
        result.extend(values);
        result
    }
}
pub struct IntoIter<T> { deque: VecDeque<T> }
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self)->Option<T> { self.deque.pop_front() }
    fn size_hint(&self)->(usize,Option<usize>) { (self.deque.len(),Some(self.deque.len())) }
}
impl<T> DoubleEndedIterator for IntoIter<T> { fn next_back(&mut self)->Option<T> { self.deque.pop_back() } }
impl<T> ExactSizeIterator for IntoIter<T> { fn len(&self)->usize { self.deque.len() } }
impl<T> core::iter::FusedIterator for IntoIter<T> {}
impl<T> IntoIterator for VecDeque<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self)->IntoIter<T> { IntoIter { deque: self } }
}
pub struct Iter<'a,T> { slots: &'a [Option<T>], front:usize, len:usize }
impl<'a,T> Clone for Iter<'a,T> { fn clone(&self)->Self { Self { slots:self.slots,front:self.front,len:self.len } } }
impl<'a,T> Iterator for Iter<'a,T> {
    type Item = &'a T;
    fn next(&mut self)->Option<&'a T> {
        if self.len==0 { return None; }
        let index=self.front;
        self.front=(self.front+1)%self.slots.len();
        self.len-=1;
        self.slots[index].as_ref()
    }
    fn size_hint(&self)->(usize,Option<usize>) { (self.len,Some(self.len)) }
}
impl<'a,T> DoubleEndedIterator for Iter<'a,T> {
    fn next_back(&mut self)->Option<&'a T> {
        if self.len==0 { return None; }
        self.len-=1;
        self.slots[(self.front+self.len)%self.slots.len()].as_ref()
    }
}
impl<T> ExactSizeIterator for Iter<'_,T> { fn len(&self)->usize { self.len } }
impl<T> core::iter::FusedIterator for Iter<'_,T> {}
impl<'a,T> IntoIterator for &'a VecDeque<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a,T>;
    fn into_iter(self)->Iter<'a,T> { self.iter() }
}
