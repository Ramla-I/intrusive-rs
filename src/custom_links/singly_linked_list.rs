// Copyright 2020 Amari Robinson
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use core::cell::Cell;
use core::fmt;
use core::ptr::NonNull;

use super::link_ops::{self, DefaultLinkOps};
use super::pointer_ops::PointerOps;
use super::Adapter;

// =============================================================================
// SinglyLinkedListOps
// =============================================================================

/// Link operations for `SinglyLinkedList`.
pub unsafe trait SinglyLinkedListOps: super::LinkOps {
    fn next(&self, ptr: Self::LinkPtr) -> Option<Self::LinkPtr>;

    unsafe fn set_next(&mut self, ptr: Self::LinkPtr, next: Option<Self::LinkPtr>);
}

// =============================================================================
// Link
// =============================================================================

/// Intrusive link that allows an object to be inserted into a
/// `SinglyLinkedList`.
pub struct Link {
    next: Cell<Option<NonNull<Link>>>,
}

// Use a special value to indicate an unlinked node
const UNLINKED_MARKER: Option<NonNull<Link>> =
    unsafe { Some(NonNull::new_unchecked(1 as *mut Link)) };

impl Link {
    /// Creates a new `Link`.
    #[inline]
    pub const fn new() -> Link {
        Link {
            next: Cell::new(UNLINKED_MARKER),
        }
    }

    /// Checks whether the `Link` is linked into a `SinglyLinkedList`.
    #[inline]
    pub fn is_linked(&self) -> bool {
        self.next.get() != UNLINKED_MARKER
    }

    /// Forcibly unlinks an object from a `SinglyLinkedList`.
    ///
    /// # Safety
    ///
    /// It is undefined behavior to call this function while still linked into a
    /// `SinglyLinkedList`. The only situation where this function is useful is
    /// after calling `fast_clear` on a `SinglyLinkedList`, since this clears
    /// the collection without marking the nodes as unlinked.
    #[inline]
    pub unsafe fn force_unlink(&self) {
        self.next.set(UNLINKED_MARKER);
    }
}

impl DefaultLinkOps for Link {
    type Ops = LinkOps;
}

// An object containing a link can be sent to another thread if it is unlinked.
unsafe impl Send for Link {}

// Provide an implementation of Clone which simply initializes the new link as
// unlinked. This allows structs containing a link to derive Clone.
impl Clone for Link {
    #[inline]
    fn clone(&self) -> Link {
        Link::new()
    }
}

// Same as above
impl Default for Link {
    #[inline]
    fn default() -> Link {
        Link::new()
    }
}

// Provide an implementation of Debug so that structs containing a link can
// still derive Debug.
impl fmt::Debug for Link {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // There isn't anything sensible to print here except whether the link
        // is currently in a list.
        if self.is_linked() {
            write!(f, "linked")
        } else {
            write!(f, "unlinked")
        }
    }
}

// =============================================================================
// LinkOps
// =============================================================================

/// Default `LinkOps` implementation for `SinglyLinkedList`.
#[derive(Clone, Copy, Default)]
pub struct LinkOps;

impl link_ops::LinkOps for LinkOps {
    type LinkPtr = NonNull<Link>;

    #[inline]
    fn is_linked(&self, ptr: Self::LinkPtr) -> bool {
        unsafe { ptr.as_ref().is_linked() }
    }

    #[inline]
    unsafe fn mark_unlinked(&mut self, ptr: Self::LinkPtr) {
        ptr.as_ref().next.set(UNLINKED_MARKER);
    }
}

unsafe impl SinglyLinkedListOps for LinkOps {
    #[inline]
    fn next(&self, ptr: Self::LinkPtr) -> Option<Self::LinkPtr> {
        unsafe { ptr.as_ref().next.get() }
    }

    #[inline]
    unsafe fn set_next(&mut self, ptr: Self::LinkPtr, next: Option<Self::LinkPtr>) {
        ptr.as_ref().next.set(next);
    }
}

#[inline]
unsafe fn link_between<T: SinglyLinkedListOps>(
    link_ops: &mut T,
    ptr: T::LinkPtr,
    prev: Option<T::LinkPtr>,
    next: Option<T::LinkPtr>,
) {
    if let Some(prev) = prev {
        link_ops.set_next(prev, Some(ptr));
    }
    link_ops.set_next(ptr, next);
}

#[inline]
unsafe fn link_after<T: SinglyLinkedListOps>(link_ops: &mut T, ptr: T::LinkPtr, prev: T::LinkPtr) {
    link_between(link_ops, ptr, Some(prev), link_ops.next(prev));
}

#[inline]
unsafe fn replace_with<T: SinglyLinkedListOps>(
    link_ops: &mut T,
    ptr: T::LinkPtr,
    prev: Option<T::LinkPtr>,
    new: T::LinkPtr,
) {
    if let Some(prev) = prev {
        link_ops.set_next(prev, Some(new));
    }
    link_ops.set_next(new, link_ops.next(ptr));
    link_ops.mark_unlinked(ptr);
}

#[inline]
unsafe fn remove<T: SinglyLinkedListOps>(
    link_ops: &mut T,
    ptr: T::LinkPtr,
    prev: Option<T::LinkPtr>,
) {
    if let Some(prev) = prev {
        link_ops.set_next(prev, link_ops.next(ptr));
    }
    link_ops.mark_unlinked(ptr);
}

#[inline]
unsafe fn splice<T: SinglyLinkedListOps>(
    link_ops: &mut T,
    start: T::LinkPtr,
    end: T::LinkPtr,
    prev: Option<T::LinkPtr>,
    next: Option<T::LinkPtr>,
) {
    link_ops.set_next(end, next);
    if let Some(prev) = prev {
        link_ops.set_next(prev, Some(start));
    }
}

// =============================================================================
// Cursor, CursorMut
// =============================================================================

/// A cursor which provides read-only access to a `SinglyLinkedList`.
pub struct Cursor<'a, A: Adapter>
where
    A::LinkOps: SinglyLinkedListOps,
{
    current: Option<<A::LinkOps as super::LinkOps>::LinkPtr>,
    list: &'a SinglyLinkedList<A>,
}

impl<'a, A: Adapter> Clone for Cursor<'a, A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    #[inline]
    fn clone(&self) -> Cursor<'a, A> {
        Cursor {
            current: self.current,
            list: self.list,
        }
    }
}

impl<'a, A: Adapter> Cursor<'a, A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    /// Checks if the cursor is currently pointing to the null object.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.current.is_none()
    }

    /// Returns a reference to the object that the cursor is currently
    /// pointing to.
    ///
    /// This returns `None` if the cursor is currently pointing to the null
    /// object.
    #[inline]
    pub fn get(&self) -> Option<&'a <A::PointerOps as PointerOps>::Value> {
        Some(unsafe { &*self.list.adapter.get_value(self.current?) })
    }

    /// Clones and returns the pointer that points to the element that the
    /// cursor is referencing.
    ///
    /// This returns `None` if the cursor is currently pointing to the null
    /// object.
    #[inline]
    pub fn clone_pointer(&self) -> Option<<A::PointerOps as PointerOps>::Pointer>
    where
        <A::PointerOps as PointerOps>::Pointer: Clone,
    {
        let raw_pointer = self.get()? as *const <A::PointerOps as PointerOps>::Value;
        Some(unsafe {
            super::pointer_ops::clone_pointer_from_raw(self.list.adapter.pointer_ops(), raw_pointer)
        })
    }

    /// Moves the cursor to the next element of the `SinglyLinkedList`.
    ///
    /// If the cursor is pointer to the null object then this will move it to
    /// the first element of the `SinglyLinkedList`. If it is pointing to the
    /// last element of the `SinglyLinkedList` then this will move it to the
    /// null object.
    #[inline]
    pub fn move_next(&mut self) {
        if let Some(current) = self.current {
            self.current = self.list.adapter.link_ops().next(current);
        } else {
            self.current = self.list.head;
        }
    }

    /// Returns a cursor pointing to the next element of the `SinglyLinkedList`.
    ///
    /// If the cursor is pointer to the null object then this will return the
    /// first element of the `SinglyLinkedList`. If it is pointing to the last
    /// element of the `SinglyLinkedList` then this will return a null cursor.
    #[inline]
    pub fn peek_next(&self) -> Cursor<'_, A> {
        let mut next = self.clone();
        next.move_next();
        next
    }
}

/// A cursor which provides mutable access to a `SinglyLinkedList`.
pub struct CursorMut<'a, A: Adapter>
where
    A::LinkOps: SinglyLinkedListOps,
{
    current: Option<<A::LinkOps as super::LinkOps>::LinkPtr>,
    list: &'a mut SinglyLinkedList<A>,
}

impl<'a, A: Adapter> CursorMut<'a, A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    /// Checks if the cursor is currently pointing to the null object.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.current.is_none()
    }

    /// Returns a reference to the object that the cursor is currently
    /// pointing to.
    ///
    /// This returns None if the cursor is currently pointing to the null
    /// object.
    #[inline]
    pub fn get(&self) -> Option<&<A::PointerOps as PointerOps>::Value> {
        Some(unsafe { &*self.list.adapter.get_value(self.current?) })
    }

    /// Returns a read-only cursor pointing to the current element.
    ///
    /// The lifetime of the returned `Cursor` is bound to that of the
    /// `CursorMut`, which means it cannot outlive the `CursorMut` and that the
    /// `CursorMut` is frozen for the lifetime of the `Cursor`.
    #[inline]
    pub fn as_cursor(&self) -> Cursor<'_, A> {
        Cursor {
            current: self.current,
            list: self.list,
        }
    }

    /// Moves the cursor to the next element of the `SinglyLinkedList`.
    ///
    /// If the cursor is pointer to the null object then this will move it to
    /// the first element of the `SinglyLinkedList`. If it is pointing to the
    /// last element of the `SinglyLinkedList` then this will move it to the
    /// null object.
    #[inline]
    pub fn move_next(&mut self) {
        if let Some(current) = self.current {
            self.current = self.list.adapter.link_ops().next(current);
        } else {
            self.current = self.list.head;
        }
    }

    /// Returns a cursor pointing to the next element of the `SinglyLinkedList`.
    ///
    /// If the cursor is pointer to the null object then this will return the
    /// first element of the `SinglyLinkedList`. If it is pointing to the last
    /// element of the `SinglyLinkedList` then this will return a null cursor.
    #[inline]
    pub fn peek_next(&self) -> Cursor<'_, A> {
        let mut next = self.as_cursor();
        next.move_next();
        next
    }

    /// Removes the next element from the `SinglyLinkedList`.
    ///
    /// A pointer to the element that was removed is returned, and the cursor is
    /// not moved.
    ///
    /// If the cursor is currently pointing to the last element of the
    /// `SinglyLinkedList` then no element is removed and `None` is returned.
    #[inline]
    pub fn remove_next(&mut self) -> Option<<A::PointerOps as PointerOps>::Pointer> {
        unsafe {
            let next = if let Some(current) = self.current {
                self.list.adapter.link_ops().next(current)
            } else {
                self.list.head
            }?;

            if self.is_null() {
                self.list.head = self.list.adapter.link_ops().next(next);
            }
            remove(self.list.adapter.link_ops_mut(), next, self.current);

            Some(
                self.list
                    .adapter
                    .pointer_ops()
                    .from_raw(self.list.adapter.get_value(next)),
            )
        }
    }

    /// Removes the next element from the `SinglyLinkedList` and inserts
    /// another object in its place.
    ///
    /// A pointer to the element that was removed is returned, and the cursor is
    /// not moved.
    ///
    /// If the cursor is currently pointing to the last element of the
    /// `SinglyLinkedList` then no element is added or removed and an error is
    /// returned containing the given `val` parameter.
    ///
    /// # Panics
    ///
    /// Panics if the new element is already linked to a different intrusive
    /// collection.
    #[inline]
    pub fn replace_next_with(
        &mut self,
        val: <A::PointerOps as PointerOps>::Pointer,
    ) -> Result<<A::PointerOps as PointerOps>::Pointer, <A::PointerOps as PointerOps>::Pointer>
    {
        unsafe {
            let next = if let Some(current) = self.current {
                self.list.adapter.link_ops().next(current)
            } else {
                self.list.head
            };
            match next {
                Some(next) => {
                    let new = self.list.node_from_value(val);
                    if self.is_null() {
                        self.list.head = Some(new);
                    }
                    replace_with(self.list.adapter.link_ops_mut(), next, self.current, new);
                    Ok(self
                        .list
                        .adapter
                        .pointer_ops()
                        .from_raw(self.list.adapter.get_value(next)))
                }
                None => Err(val),
            }
        }
    }

    /// Inserts a new element into the `SinglyLinkedList` after the current one.
    ///
    /// If the cursor is pointing at the null object then the new element is
    /// inserted at the front of the `SinglyLinkedList`.
    ///
    /// # Panics
    ///
    /// Panics if the new element is already linked to a different intrusive
    /// collection.
    #[inline]
    pub fn insert_after(&mut self, val: <A::PointerOps as PointerOps>::Pointer) {
        unsafe {
            let new = self.list.node_from_value(val);
            if let Some(current) = self.current {
                link_after(self.list.adapter.link_ops_mut(), new, current);
            } else {
                link_between(self.list.adapter.link_ops_mut(), new, None, self.list.head);
                self.list.head = Some(new);
            }
        }
    }

    /// Inserts the elements from the given `SinglyLinkedList` after the current
    /// one.
    ///
    /// If the cursor is pointing at the null object then the new elements are
    /// inserted at the start of the `SinglyLinkedList`.
    ///
    /// Note that if the cursor is not pointing to the last element of the
    /// `SinglyLinkedList` then the given list must be scanned to find its last
    /// element. This has linear time complexity.
    #[inline]
    pub fn splice_after(&mut self, mut list: SinglyLinkedList<A>) {
        if let Some(head) = list.head {
            unsafe {
                let next = if let Some(current) = self.current {
                    self.list.adapter.link_ops().next(current)
                } else {
                    self.list.head
                };
                if let Some(next) = next {
                    let mut tail = head;
                    while let Some(x) = self.list.adapter.link_ops().next(tail) {
                        tail = x;
                    }
                    splice(
                        self.list.adapter.link_ops_mut(),
                        head,
                        tail,
                        self.current,
                        Some(next),
                    );
                    if self.is_null() {
                        self.list.head = list.head;
                    }
                } else {
                    if let Some(current) = self.current {
                        self.list
                            .adapter
                            .link_ops_mut()
                            .set_next(current, list.head);
                    } else {
                        self.list.head = list.head;
                    }
                }
                list.head = None;
            }
        }
    }

    /// Splits the list into two after the current element. This will return a
    /// new list consisting of everything after the cursor, with the original
    /// list retaining everything before.
    ///
    /// If the cursor is pointing at the null object then the entire contents
    /// of the `SinglyLinkedList` are moved.
    #[inline]
    pub fn split_after(&mut self) -> SinglyLinkedList<A>
    where
        A: Clone,
    {
        if let Some(current) = self.current {
            unsafe {
                let list = SinglyLinkedList {
                    head: self.list.adapter.link_ops().next(current),
                    adapter: self.list.adapter.clone(),
                };
                self.list.adapter.link_ops_mut().set_next(current, None);
                list
            }
        } else {
            let list = SinglyLinkedList {
                head: self.list.head,
                adapter: self.list.adapter.clone(),
            };
            self.list.head = None;
            list
        }
    }
}

// =============================================================================
// SinglyLinkedList
// =============================================================================

/// An intrusive singly-linked list.
///
/// When this collection is dropped, all elements linked into it will be
/// converted back to owned pointers and dropped.
pub struct SinglyLinkedList<A: Adapter>
where
    A::LinkOps: SinglyLinkedListOps,
{
    head: Option<<A::LinkOps as super::LinkOps>::LinkPtr>,
    adapter: A,
}

impl<A: Adapter> SinglyLinkedList<A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    #[inline]
    fn node_from_value(
        &self,
        val: <A::PointerOps as PointerOps>::Pointer,
    ) -> <A::LinkOps as super::LinkOps>::LinkPtr {
        use link_ops::LinkOps;

        unsafe {
            let raw = self.adapter.pointer_ops().into_raw(val);

            if self
                .adapter
                .link_ops()
                .is_linked(self.adapter.get_link(raw))
            {
                // convert the node back into a pointer
                self.adapter.pointer_ops().from_raw(raw);

                panic!("attempted to insert an object that is already linked");
            }

            self.adapter.get_link(raw)
        }
    }

    /// Creates an empty `SinglyLinkedList`.
    #[inline]
    pub fn new(adapter: A) -> SinglyLinkedList<A> {
        SinglyLinkedList {
            head: None,
            adapter,
        }
    }

    /// Returns `true` if the `SinglyLinkedList` is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Returns a null `Cursor` for this list.
    pub fn cursor(&self) -> Cursor<'_, A> {
        Cursor {
            current: None,
            list: self,
        }
    }

    /// Returns a null `CursorMut` for this list.
    pub fn cursor_mut(&mut self) -> CursorMut<'_, A> {
        CursorMut {
            current: None,
            list: self,
        }
    }

    /// Creates a `Cursor` from a pointer to an element.
    ///
    /// # Safety
    ///
    /// `ptr` must be a pointer to an object that is part of this list.
    pub unsafe fn cursor_from_ptr(
        &self,
        ptr: *const <A::PointerOps as PointerOps>::Value,
    ) -> Cursor<'_, A> {
        Cursor {
            current: Some(self.adapter.get_link(ptr)),
            list: self,
        }
    }

    /// Creates a `CursorMut` from a pointer to an element.
    ///
    /// # Safety
    ///
    /// `ptr` must be a pointer to an object that is part of this list.
    pub unsafe fn cursor_mut_from_ptr(
        &mut self,
        ptr: *const <A::PointerOps as PointerOps>::Value,
    ) -> CursorMut<'_, A> {
        CursorMut {
            current: Some(self.adapter.get_link(ptr)),
            list: self,
        }
    }

    /// Returns a `Cursor` pointing to the first element of the list. If the
    /// list is empty then a null cursor is returned.
    pub fn front(&self) -> Cursor<'_, A> {
        let mut cursor = self.cursor();
        cursor.move_next();
        cursor
    }

    /// Returns a `CursorMut` pointing to the first element of the list. If the
    /// the list is empty then a null cursor is returned.
    pub fn front_mut(&mut self) -> CursorMut<'_, A> {
        let mut cursor = self.cursor_mut();
        cursor.move_next();
        cursor
    }

    /// Gets an iterator over the objects in the `SinglyLinkedList`.
    #[inline]
    pub fn iter(&self) -> Iter<'_, A> {
        Iter {
            current: self.head,
            list: self,
        }
    }

    /// Removes all elements from the `SinglyLinkedList`.
    ///
    /// This will unlink all object currently in the list, which requires
    /// iterating through all elements in the `SinglyLinkedList`. Each element is
    /// converted back to an owned pointer and then dropped.
    #[inline]
    pub fn clear(&mut self) {
        use link_ops::LinkOps;

        let mut current = self.head;
        self.head = None;
        while let Some(x) = current {
            unsafe {
                let next = self.adapter.link_ops().next(x);
                self.adapter.link_ops_mut().mark_unlinked(x);
                self.adapter
                    .pointer_ops()
                    .from_raw(self.adapter.get_value(x));
                current = next;
            }
        }
    }

    /// Empties the `SinglyLinkedList` without unlinking or freeing objects in it.
    ///
    /// Since this does not unlink any objects, any attempts to link these
    /// objects into another `SinglyLinkedList` will fail but will not cause any
    /// memory unsafety. To unlink those objects manually, you must call the
    /// `force_unlink` function on them.
    pub fn fast_clear(&mut self) {
        self.head = None;
    }

    /// Takes all the elements out of the `SinglyLinkedList`, leaving it empty.
    /// The taken elements are returned as a new `SinglyLinkedList`.
    pub fn take(&mut self) -> SinglyLinkedList<A>
    where
        A: Clone,
    {
        let list = SinglyLinkedList {
            head: self.head,
            adapter: self.adapter.clone(),
        };
        self.head = None;
        list
    }

    /// Inserts a new element at the start of the `SinglyLinkedList`.
    #[inline]
    pub fn push_front(&mut self, val: <A::PointerOps as PointerOps>::Pointer) {
        self.cursor_mut().insert_after(val);
    }

    /// Removes the first element of the `SinglyLinkedList`.
    ///
    /// This returns `None` if the `SinglyLinkedList` is empty.
    #[inline]
    pub fn pop_front(&mut self) -> Option<<A::PointerOps as PointerOps>::Pointer> {
        self.cursor_mut().remove_next()
    }
}

// Allow read-only access to values from multiple threads
unsafe impl<A: Adapter + Sync> Sync for SinglyLinkedList<A>
where
    <A::PointerOps as PointerOps>::Value: Sync,
    A::LinkOps: SinglyLinkedListOps,
{
}

// Allow sending to another thread if the ownership (represented by the <A::PointerOps as PointerOps>::Pointer owned
// pointer type) can be transferred to another thread.
unsafe impl<A: Adapter + Send> Send for SinglyLinkedList<A>
where
    <A::PointerOps as PointerOps>::Pointer: Send,
    A::LinkOps: SinglyLinkedListOps,
{
}

// Drop all owned pointers if the collection is dropped
impl<A: Adapter> Drop for SinglyLinkedList<A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    #[inline]
    fn drop(&mut self) {
        self.clear();
    }
}

impl<A: Adapter> IntoIterator for SinglyLinkedList<A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    type Item = <A::PointerOps as PointerOps>::Pointer;
    type IntoIter = IntoIter<A>;

    #[inline]
    fn into_iter(self) -> IntoIter<A> {
        IntoIter { list: self }
    }
}

impl<'a, A: Adapter + 'a> IntoIterator for &'a SinglyLinkedList<A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    type Item = &'a <A::PointerOps as PointerOps>::Value;
    type IntoIter = Iter<'a, A>;

    #[inline]
    fn into_iter(self) -> Iter<'a, A> {
        self.iter()
    }
}

impl<A: Adapter + Default> Default for SinglyLinkedList<A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    fn default() -> SinglyLinkedList<A> {
        SinglyLinkedList::new(A::default())
    }
}

impl<A: Adapter> fmt::Debug for SinglyLinkedList<A>
where
    A::LinkOps: SinglyLinkedListOps,
    <A::PointerOps as PointerOps>::Value: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

// =============================================================================
// Iter
// =============================================================================

/// An iterator over references to the items of a `SinglyLinkedList`.
pub struct Iter<'a, A: Adapter>
where
    A::LinkOps: SinglyLinkedListOps,
{
    current: Option<<A::LinkOps as super::LinkOps>::LinkPtr>,
    list: &'a SinglyLinkedList<A>,
}
impl<'a, A: Adapter + 'a> Iterator for Iter<'a, A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    type Item = &'a <A::PointerOps as PointerOps>::Value;

    #[inline]
    fn next(&mut self) -> Option<&'a <A::PointerOps as PointerOps>::Value> {
        let current = self.current?;

        self.current = self.list.adapter.link_ops().next(current);
        Some(unsafe { &*self.list.adapter.get_value(current) })
    }
}
impl<'a, A: Adapter + 'a> Clone for Iter<'a, A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    #[inline]
    fn clone(&self) -> Iter<'a, A> {
        Iter {
            current: self.current,
            list: self.list,
        }
    }
}

// =============================================================================
// IntoIter
// =============================================================================

/// An iterator which consumes a `SinglyLinkedList`.
pub struct IntoIter<A: Adapter>
where
    A::LinkOps: SinglyLinkedListOps,
{
    list: SinglyLinkedList<A>,
}
impl<A: Adapter> Iterator for IntoIter<A>
where
    A::LinkOps: SinglyLinkedListOps,
{
    type Item = <A::PointerOps as PointerOps>::Pointer;

    #[inline]
    fn next(&mut self) -> Option<<A::PointerOps as PointerOps>::Pointer> {
        self.list.pop_front()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::{link_ops, Adapter, DefaultLinkOps, Link, LinkOps, PointerOps, SinglyLinkedList};
    use crate::custom_links::pointer_ops::DefaultPointerOps;
    use crate::UnsafeRef;
    use core::ptr::NonNull;
    use std::boxed::Box;
    use std::fmt;
    use std::vec::Vec;

    struct Obj {
        link1: Link,
        link2: Link,
        value: u32,
    }
    impl fmt::Debug for Obj {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.value)
        }
    }
    struct ObjAdapter1(
        LinkOps,
        DefaultPointerOps<UnsafeRef<Obj>>,
        core::marker::PhantomData<UnsafeRef<Obj>>,
    );
    unsafe impl Send for ObjAdapter1 {}
    unsafe impl Sync for ObjAdapter1 {}
    impl Clone for ObjAdapter1 {
        #[inline]
        fn clone(&self) -> Self {
            *self
        }
    }
    impl Copy for ObjAdapter1 {}
    impl Default for ObjAdapter1 {
        #[inline]
        fn default() -> Self {
            Self::new()
        }
    }
    #[allow(dead_code)]
    impl ObjAdapter1 {
        pub const NEW: Self =
            ObjAdapter1(LinkOps, DefaultPointerOps::new(), core::marker::PhantomData);
        #[inline]
        pub fn new() -> Self {
            Self::NEW
        }
    }
    #[allow(dead_code, unsafe_code)]
    unsafe impl Adapter for ObjAdapter1 {
        type LinkOps = LinkOps;
        type PointerOps = DefaultPointerOps<UnsafeRef<Obj>>;

        #[inline]
        unsafe fn get_value(
            &self,
            link: <Self::LinkOps as link_ops::LinkOps>::LinkPtr,
        ) -> *const <Self::PointerOps as PointerOps>::Value {
            container_of!(link.as_ptr(), Obj, link1)
        }
        #[inline]
        unsafe fn get_link(
            &self,
            value: *const <Self::PointerOps as PointerOps>::Value,
        ) -> <Self::LinkOps as link_ops::LinkOps>::LinkPtr {
            NonNull::new_unchecked(&(*value).link1 as *const Link as *mut Link)
        }

        #[inline]
        fn link_ops(&self) -> &Self::LinkOps {
            &self.0
        }

        #[inline]
        fn link_ops_mut(&mut self) -> &mut Self::LinkOps {
            &mut self.0
        }

        #[inline]
        fn pointer_ops(&self) -> &Self::PointerOps {
            &self.1
        }
    }
    struct ObjAdapter2(
        LinkOps,
        DefaultPointerOps<UnsafeRef<Obj>>,
        core::marker::PhantomData<UnsafeRef<Obj>>,
    );
    unsafe impl Send for ObjAdapter2 {}
    unsafe impl Sync for ObjAdapter2 {}
    impl Clone for ObjAdapter2 {
        #[inline]
        fn clone(&self) -> Self {
            *self
        }
    }
    impl Copy for ObjAdapter2 {}
    impl Default for ObjAdapter2 {
        #[inline]
        fn default() -> Self {
            Self::new()
        }
    }
    #[allow(dead_code)]
    impl ObjAdapter2 {
        pub const NEW: Self =
            ObjAdapter2(LinkOps, DefaultPointerOps::new(), core::marker::PhantomData);
        #[inline]
        pub fn new() -> Self {
            Self::NEW
        }
    }
    #[allow(dead_code, unsafe_code)]
    unsafe impl Adapter for ObjAdapter2 {
        type LinkOps = LinkOps;
        type PointerOps = DefaultPointerOps<UnsafeRef<Obj>>;

        #[inline]
        unsafe fn get_value(
            &self,
            link: <Self::LinkOps as link_ops::LinkOps>::LinkPtr,
        ) -> *const <Self::PointerOps as PointerOps>::Value {
            container_of!(link.as_ptr(), Obj, link2)
        }
        #[inline]
        unsafe fn get_link(
            &self,
            value: *const <Self::PointerOps as PointerOps>::Value,
        ) -> <Self::LinkOps as link_ops::LinkOps>::LinkPtr {
            NonNull::new_unchecked(&(*value).link2 as *const Link as *mut Link)
        }

        #[inline]
        fn link_ops(&self) -> &Self::LinkOps {
            &self.0
        }

        #[inline]
        fn link_ops_mut(&mut self) -> &mut Self::LinkOps {
            &mut self.0
        }

        #[inline]
        fn pointer_ops(&self) -> &Self::PointerOps {
            &self.1
        }
    }
    fn make_obj(value: u32) -> UnsafeRef<Obj> {
        UnsafeRef::from_box(Box::new(Obj {
            link1: Link::new(),
            link2: Link::default(),
            value: value,
        }))
    }

    #[test]
    fn test_link() {
        let a = make_obj(1);
        assert!(!a.link1.is_linked());
        assert!(!a.link2.is_linked());

        let mut b = SinglyLinkedList::<ObjAdapter1>::default();
        assert!(b.is_empty());

        b.push_front(a.clone());
        assert!(!b.is_empty());
        assert!(a.link1.is_linked());
        assert!(!a.link2.is_linked());
        assert_eq!(format!("{:?}", a.link1), "linked");
        assert_eq!(format!("{:?}", a.link2), "unlinked");

        assert_eq!(
            b.pop_front().unwrap().as_ref() as *const _,
            a.as_ref() as *const _
        );
        assert!(b.is_empty());
        assert!(!a.link1.is_linked());
        assert!(!a.link2.is_linked());
    }

    #[test]
    fn test_cursor() {
        let a = make_obj(1);
        let b = make_obj(2);
        let c = make_obj(3);

        let mut l = SinglyLinkedList::new(ObjAdapter1::new());
        let mut cur = l.cursor_mut();
        assert!(cur.is_null());
        assert!(cur.get().is_none());
        assert!(cur.remove_next().is_none());
        assert_eq!(
            cur.replace_next_with(a.clone()).unwrap_err().as_ref() as *const _,
            a.as_ref() as *const _
        );

        cur.insert_after(c.clone());
        cur.insert_after(a.clone());
        cur.move_next();
        cur.insert_after(b.clone());
        cur.move_next();
        cur.move_next();
        assert!(cur.peek_next().is_null());
        cur.move_next();
        assert!(cur.is_null());

        cur.move_next();
        assert!(!cur.is_null());
        assert_eq!(cur.get().unwrap() as *const _, a.as_ref() as *const _);

        {
            let mut cur2 = cur.as_cursor();
            assert_eq!(cur2.get().unwrap() as *const _, a.as_ref() as *const _);
            assert_eq!(cur2.peek_next().get().unwrap().value, 2);
            cur2.move_next();
            assert_eq!(cur2.get().unwrap().value, 2);
            cur2.move_next();
            assert_eq!(cur2.get().unwrap() as *const _, c.as_ref() as *const _);
            cur2.move_next();
            assert!(cur2.is_null());
            assert!(cur2.clone().get().is_none());
        }
        assert_eq!(cur.get().unwrap() as *const _, a.as_ref() as *const _);

        assert_eq!(
            cur.remove_next().unwrap().as_ref() as *const _,
            b.as_ref() as *const _
        );
        assert_eq!(cur.get().unwrap() as *const _, a.as_ref() as *const _);
        cur.insert_after(b.clone());
        assert_eq!(cur.get().unwrap() as *const _, a.as_ref() as *const _);
        cur.move_next();
        assert_eq!(cur.get().unwrap() as *const _, b.as_ref() as *const _);
        assert_eq!(
            cur.remove_next().unwrap().as_ref() as *const _,
            c.as_ref() as *const _
        );
        assert!(!c.link1.is_linked());
        assert!(a.link1.is_linked());
        assert_eq!(cur.get().unwrap() as *const _, b.as_ref() as *const _);
        cur.move_next();
        assert!(cur.is_null());
        assert_eq!(
            cur.replace_next_with(c.clone()).unwrap().as_ref() as *const _,
            a.as_ref() as *const _
        );
        assert!(!a.link1.is_linked());
        assert!(c.link1.is_linked());
        assert!(cur.is_null());
        cur.move_next();
        assert_eq!(cur.get().unwrap() as *const _, c.as_ref() as *const _);
        assert_eq!(
            cur.replace_next_with(a.clone()).unwrap().as_ref() as *const _,
            b.as_ref() as *const _
        );
        assert!(a.link1.is_linked());
        assert!(!b.link1.is_linked());
        assert!(c.link1.is_linked());
        assert_eq!(cur.get().unwrap() as *const _, c.as_ref() as *const _);
    }

    #[test]
    fn test_split_splice() {
        let mut l1 = SinglyLinkedList::new(ObjAdapter1::new());
        let mut l2 = SinglyLinkedList::new(ObjAdapter1::new());
        let mut l3 = SinglyLinkedList::new(ObjAdapter1::new());

        let a = make_obj(1);
        let b = make_obj(2);
        let c = make_obj(3);
        let d = make_obj(4);
        l1.cursor_mut().insert_after(d.clone());
        l1.cursor_mut().insert_after(c.clone());
        l1.cursor_mut().insert_after(b.clone());
        l1.cursor_mut().insert_after(a.clone());
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        {
            let mut cur = l1.front_mut();
            cur.move_next();
            l2 = cur.split_after();
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [3, 4]);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        {
            let mut cur = l2.front_mut();
            l3 = cur.split_after();
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [3]);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), [4]);
        {
            let mut cur = l1.front_mut();
            cur.splice_after(l2.take());
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), [1, 3, 2]);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), [4]);
        {
            let mut cur = l1.cursor_mut();
            cur.splice_after(l3.take());
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), [4, 1, 3, 2]);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        {
            let mut cur = l1.cursor_mut();
            l2 = cur.split_after();
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [4, 1, 3, 2]);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        {
            let mut cur = l2.front_mut();
            cur.move_next();
            l3 = cur.split_after();
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [4, 1]);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), [3, 2]);
        {
            let mut cur = l2.front_mut();
            cur.splice_after(l3.take());
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [4, 3, 2, 1]);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        {
            let mut cur = l3.cursor_mut();
            cur.splice_after(l2.take());
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), [4, 3, 2, 1]);
        {
            let mut cur = l3.front_mut();
            cur.move_next();
            l2 = cur.split_after();
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), [4, 3]);
        {
            let mut cur = l2.front_mut();
            cur.move_next();
            cur.splice_after(l3.take());
        }
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), []);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [2, 1, 4, 3]);
        assert_eq!(l3.iter().map(|x| x.value).collect::<Vec<_>>(), []);
    }

    #[test]
    fn test_iter() {
        let mut l = SinglyLinkedList::new(ObjAdapter1::new());
        let a = make_obj(1);
        let b = make_obj(2);
        let c = make_obj(3);
        let d = make_obj(4);
        l.cursor_mut().insert_after(d.clone());
        l.cursor_mut().insert_after(c.clone());
        l.cursor_mut().insert_after(b.clone());
        l.cursor_mut().insert_after(a.clone());

        assert_eq!(l.front().get().unwrap().value, 1);
        unsafe {
            assert_eq!(l.cursor_from_ptr(b.as_ref()).get().unwrap().value, 2);
            assert_eq!(l.cursor_mut_from_ptr(c.as_ref()).get().unwrap().value, 3);
        }

        let mut v = Vec::new();
        for x in &l {
            v.push(x.value);
        }
        assert_eq!(v, [1, 2, 3, 4]);
        assert_eq!(
            l.iter().clone().map(|x| x.value).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(l.iter().map(|x| x.value).collect::<Vec<_>>(), [1, 2, 3, 4]);

        assert_eq!(format!("{:?}", l), "[1, 2, 3, 4]");

        let mut v = Vec::new();
        for x in l.take() {
            v.push(x.value);
        }
        assert_eq!(v, [1, 2, 3, 4]);
        assert!(l.is_empty());
        assert!(!a.link1.is_linked());
        assert!(!b.link1.is_linked());
        assert!(!c.link1.is_linked());
        assert!(!d.link1.is_linked());

        l.cursor_mut().insert_after(d.clone());
        l.cursor_mut().insert_after(c.clone());
        l.cursor_mut().insert_after(b.clone());
        l.cursor_mut().insert_after(a.clone());
        l.clear();
        assert!(l.is_empty());
        assert!(!a.link1.is_linked());
        assert!(!b.link1.is_linked());
        assert!(!c.link1.is_linked());
        assert!(!d.link1.is_linked());
    }

    #[test]
    fn test_multi_list() {
        let mut l1 = SinglyLinkedList::new(ObjAdapter1::new());
        let mut l2 = SinglyLinkedList::new(ObjAdapter2::new());
        let a = make_obj(1);
        let b = make_obj(2);
        let c = make_obj(3);
        let d = make_obj(4);
        l1.cursor_mut().insert_after(d.clone());
        l1.cursor_mut().insert_after(c.clone());
        l1.cursor_mut().insert_after(b.clone());
        l1.cursor_mut().insert_after(a.clone());
        l2.cursor_mut().insert_after(a.clone());
        l2.cursor_mut().insert_after(b.clone());
        l2.cursor_mut().insert_after(c.clone());
        l2.cursor_mut().insert_after(d.clone());
        assert_eq!(l1.iter().map(|x| x.value).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(l2.iter().map(|x| x.value).collect::<Vec<_>>(), [4, 3, 2, 1]);
    }

    #[test]
    fn test_fast_clear() {
        let mut l = SinglyLinkedList::new(ObjAdapter1::new());
        let a = make_obj(1);
        let b = make_obj(2);
        let c = make_obj(3);
        l.cursor_mut().insert_after(a.clone());
        l.cursor_mut().insert_after(b.clone());
        l.cursor_mut().insert_after(c.clone());

        l.fast_clear();
        assert!(l.is_empty());
        assert!(a.link1.is_linked());
        assert!(b.link1.is_linked());
        assert!(c.link1.is_linked());
        unsafe {
            a.link1.force_unlink();
            b.link1.force_unlink();
            c.link1.force_unlink();
        }
        assert!(l.is_empty());
        assert!(!a.link1.is_linked());
        assert!(!b.link1.is_linked());
        assert!(!c.link1.is_linked());
    }

    #[test]
    fn test_non_static() {
        #[derive(Clone)]
        struct Obj<'a, T> {
            link: Link,
            value: &'a T,
        }
        struct ObjAdapter<'a, T>(
            LinkOps,
            DefaultPointerOps<&'a Obj<'a, T>>,
            core::marker::PhantomData<&'a Obj<'a, T>>,
        );
        unsafe impl<'a, T> Send for ObjAdapter<'a, T> where T: 'a {}
        unsafe impl<'a, T> Sync for ObjAdapter<'a, T> where T: 'a {}
        impl<'a, T> Clone for ObjAdapter<'a, T>
        where
            T: 'a,
        {
            #[inline]
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<'a, T> Copy for ObjAdapter<'a, T> where T: 'a {}
        impl<'a, T> Default for ObjAdapter<'a, T>
        where
            T: 'a,
        {
            #[inline]
            fn default() -> Self {
                Self::new()
            }
        }
        #[allow(dead_code)]
        impl<'a, T> ObjAdapter<'a, T>
        where
            T: 'a,
        {
            pub const NEW: Self =
                ObjAdapter(LinkOps, DefaultPointerOps::new(), core::marker::PhantomData);
            #[inline]
            pub fn new() -> Self {
                Self::NEW
            }
        }
        #[allow(dead_code, unsafe_code)]
        unsafe impl<'a, T: 'a> Adapter for ObjAdapter<'a, T> {
            type LinkOps = LinkOps;
            type PointerOps = DefaultPointerOps<&'a Obj<'a, T>>;

            #[inline]
            unsafe fn get_value(
                &self,
                link: <Self::LinkOps as link_ops::LinkOps>::LinkPtr,
            ) -> *const <Self::PointerOps as PointerOps>::Value {
                container_of!(link.as_ptr(), Obj<'a, T>, link)
            }
            #[inline]
            unsafe fn get_link(
                &self,
                value: *const <Self::PointerOps as PointerOps>::Value,
            ) -> <Self::LinkOps as link_ops::LinkOps>::LinkPtr {
                NonNull::new_unchecked(&(*value).link as *const Link as *mut Link)
            }

            #[inline]
            fn link_ops(&self) -> &Self::LinkOps {
                &self.0
            }

            #[inline]
            fn link_ops_mut(&mut self) -> &mut Self::LinkOps {
                &mut self.0
            }

            #[inline]
            fn pointer_ops(&self) -> &Self::PointerOps {
                &self.1
            }
        }

        let v = 5;
        let a = Obj {
            link: Link::new(),
            value: &v,
        };
        let b = a.clone();
        let mut l = SinglyLinkedList::new(ObjAdapter::new());
        l.cursor_mut().insert_after(&a);
        l.cursor_mut().insert_after(&b);
        assert_eq!(*l.front().get().unwrap().value, 5);
        assert_eq!(*l.front().get().unwrap().value, 5);
    }

    macro_rules! test_clone_pointer {
        ($ptr: ident, $ptr_import: path) => {
            use $ptr_import;

            #[derive(Clone)]
            struct Obj {
                link: Link,
                value: usize,
            }
            struct ObjAdapter(
                LinkOps,
                DefaultPointerOps<$ptr<Obj>>,
                core::marker::PhantomData<$ptr<Obj>>,
            );
            unsafe impl Send for ObjAdapter {}
            unsafe impl Sync for ObjAdapter {}
            impl Clone for ObjAdapter {
                #[inline]
                fn clone(&self) -> Self {
                    *self
                }
            }
            impl Copy for ObjAdapter {}
            impl Default for ObjAdapter {
                #[inline]
                fn default() -> Self {
                    Self::new()
                }
            }
            #[allow(dead_code)]
            impl ObjAdapter {
                pub const NEW: Self =
                    ObjAdapter(LinkOps, DefaultPointerOps::new(), core::marker::PhantomData);
                #[inline]
                pub fn new() -> Self {
                    Self::NEW
                }
            }
            #[allow(dead_code, unsafe_code)]
            unsafe impl Adapter for ObjAdapter {
                type LinkOps = LinkOps;
                type PointerOps = DefaultPointerOps<$ptr<Obj>>;

                #[inline]
                unsafe fn get_value(
                    &self,
                    link: <Self::LinkOps as link_ops::LinkOps>::LinkPtr,
                ) -> *const <Self::PointerOps as PointerOps>::Value {
                    container_of!(link.as_ptr(), Obj, link)
                }
                #[inline]
                unsafe fn get_link(
                    &self,
                    value: *const <Self::PointerOps as PointerOps>::Value,
                ) -> <Self::LinkOps as link_ops::LinkOps>::LinkPtr {
                    NonNull::new_unchecked(&(*value).link as *const Link as *mut Link)
                }

                #[inline]
                fn link_ops(&self) -> &Self::LinkOps {
                    &self.0
                }

                #[inline]
                fn link_ops_mut(&mut self) -> &mut Self::LinkOps {
                    &mut self.0
                }

                #[inline]
                fn pointer_ops(&self) -> &Self::PointerOps {
                    &self.1
                }
            }

            let a = $ptr::new(Obj {
                link: Link::new(),
                value: 5,
            });
            let mut l = SinglyLinkedList::new(ObjAdapter::new());
            l.cursor_mut().insert_after(a.clone());
            assert_eq!(2, $ptr::strong_count(&a));

            let pointer = l.front().clone_pointer().unwrap();
            assert_eq!(pointer.value, 5);
            assert_eq!(3, $ptr::strong_count(&a));

            l.clear();
            assert!(l.front().clone_pointer().is_none());
        };
    }

    #[test]
    fn test_clone_pointer_rc() {
        test_clone_pointer!(Rc, std::rc::Rc);
    }

    #[test]
    fn test_clone_pointer_arc() {
        test_clone_pointer!(Arc, std::sync::Arc);
    }
}
