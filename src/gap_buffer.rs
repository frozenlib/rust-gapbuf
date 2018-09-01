use std::alloc::*;
use std::cmp::*;
use std::fmt::{Debug, Error, Formatter};
use std::hash::*;
use std::iter::*;
use std::marker::PhantomData;
use std::mem::*;
use std::ops::{Deref, DerefMut, Drop, FnMut, Index, IndexMut, RangeBounds};
use std::ptr;
use std::ptr::*;
use std::slice;

/// Creates a [`GapBuffer`] containing the arguments.
///
/// `gap_buffer!` allows [`GapBuffer`] to be defined with the same syntax as `vec!`.
/// There are two forms of this macro:
///
/// - Create a [`GapBuffer`] containing a given list of elements:
///
/// ```
/// # #[macro_use] extern crate gapbuf;
/// # fn main() {
///   let b = gap_buffer![1, 2, 3];
///   assert_eq!(b.len(), 3);
///   assert_eq!(b[0], 1);
///   assert_eq!(b[1], 2);
///   assert_eq!(b[2], 3);
/// # }
/// ```
///
/// - Create a [`GapBuffer`] from a given element and size:
///
/// ```
/// # #[macro_use] extern crate gapbuf;
/// # fn main() {
///   let b = gap_buffer!["abc"; 2];
///   assert_eq!(b.len(), 2);
///   assert_eq!(b[0], "abc");
///   assert_eq!(b[1], "abc");
/// # }
/// ```
///
/// [`GapBuffer`]: ../gapbuf/struct.GapBuffer.html
#[macro_export]
macro_rules! gap_buffer {
    ($elem:expr; $n:expr) => (
        {
            let mut buf = $crate::GapBuffer::with_capacity($n);
            buf.resize($n, $elem);
            buf
        }
    );
    ($($x:expr),*) => (
        {
            let mut buf = $crate::GapBuffer::new();
            $(
                buf.push_back($x);
            )*
            buf
        }
    );
    ($($x:expr,)*) => (gap_buffer![$($x),*])
}

/// Dynamic array that allows efficient insertion and deletion operations clustered near the same location.
///
/// `GapBuffer<T>` has a member similer to `Vec`.
#[derive(Hash)]
pub struct GapBuffer<T>(RawGapBuffer<T>);

impl<T> GapBuffer<T> {
    /// Constructs a new, empty `GapBuffer<T>`.
    ///
    /// The gap buffer will not allocate until elements are pushed onto it.
    ///
    /// # Examples
    /// ```
    /// # use gapbuf::GapBuffer;
    /// let mut buf = GapBuffer::<i32>::new();
    ///
    /// assert_eq!(buf.is_empty(), true);
    /// assert_eq!(buf.len(), 0);
    /// assert_eq!(buf.capacity(), 0);
    /// ```
    ///
    /// ```
    /// use gapbuf::GapBuffer;
    ///
    /// let mut buf = GapBuffer::new();
    /// buf.push_back(5);
    /// ```
    #[inline]
    pub fn new() -> Self {
        GapBuffer(RawGapBuffer::new())
    }

    /// Constructs a new, empty `GapBuffer<T>` with the specified capacity.
    ///
    /// # Examples
    /// ```
    /// use gapbuf::GapBuffer;
    ///
    /// let buf: GapBuffer<i32> = GapBuffer::with_capacity(5);
    /// assert_eq!(buf.is_empty(), true);
    /// assert_eq!(buf.len(), 0);
    /// assert_eq!(buf.capacity(), 5);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        let mut buf = GapBuffer::new();
        buf.reserve_exact(capacity);
        buf
    }

    /// Returns the number of elements the `GapBuffer<T>` can hold without reallocating.
    ///
    /// # Examples
    /// ```
    /// use gapbuf::GapBuffer;
    ///
    /// let buf: GapBuffer<i32> = GapBuffer::with_capacity(10);
    /// assert_eq!(buf.capacity(), 10);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Reserves capacity for at least additional more elements to be inserted in the given `GapBuffer<T>`.
    /// The collection may reserve more space to avoid frequent reallocations.
    /// After calling reserve, capacity will be greater than or equal to `self.len() + additional`.
    /// Does nothing if capacity is already sufficient.
    ///
    /// # Panics
    /// Panics if the new capacity overflows usize.
    ///
    /// # Examples
    /// ```
    /// use gapbuf::GapBuffer;
    ///
    /// let mut buf = GapBuffer::new();
    /// buf.push_back(1);
    /// buf.reserve(10);
    /// assert!(buf.capacity() >= 11);
    /// ```
    pub fn reserve(&mut self, additional: usize) {
        self.reserve_as(additional, false);
    }

    /// Reserves the minimum capacity for exactly additional more elements to be inserted in the given `GapBuffer<T>`.
    /// After calling reserve_exact, capacity will be greater than or equal to self.len() + additional.
    /// Does nothing if the capacity is already sufficient.
    ///
    /// Note that the allocator may give the collection more space than it requests.
    /// Therefore capacity can not be relied upon to be precisely minimal.
    /// Prefer [`reserve`] if future insertions are expected.
    ///
    /// # Panics
    /// Panics if the new capacity overflows usize.
    ///
    /// # Examples
    /// ```
    /// use gapbuf::GapBuffer;
    ///
    /// let mut buf = GapBuffer::new();
    /// buf.push_back(1);
    /// buf.reserve_exact(10);
    /// assert!(buf.capacity() >= 11);
    /// ```
    ///
    /// [`reserve`]: #method.reserve
    pub fn reserve_exact(&mut self, additional: usize) {
        self.reserve_as(additional, true);
    }
    fn reserve_as(&mut self, additional: usize, exact: bool) {
        let len = self.len();
        let old_cap = self.cap;
        let new_cap = len.checked_add(additional).expect("capacity overflow");
        if new_cap <= old_cap {
            return;
        }
        let new_cap = if exact {
            new_cap
        } else {
            max(old_cap.saturating_mul(2), new_cap)
        };
        self.0.realloc(new_cap);

        unsafe {
            let p = self.as_mut_ptr();
            let count = len - self.gap();
            copy(p.add(old_cap - count), p.add(new_cap - count), count);
        }
    }

    /// Shrinks the capacity of the `GapBuffer<T>` as much as possible.
    ///
    /// # Examples
    /// ```
    /// use gapbuf::GapBuffer;
    ///
    /// let mut buf = GapBuffer::new();
    /// buf.push_back(1);
    ///
    /// buf.reserve(10);
    /// assert!(buf.capacity() >= 11);
    ///
    /// buf.shrink_to_fit();
    /// assert_eq!(buf.capacity(), 1);
    /// ```
    pub fn shrink_to_fit(&mut self) {
        let len = self.len();
        self.set_gap(len);
        self.0.realloc(len);
    }

    /// Set gap offset of the `GapBuffer<T>`.
    ///
    /// # Panics
    /// Panics if `index > len`.
    ///
    /// # Computational amount
    /// `O(n)` , `n = |self.gap() - gap|`
    #[inline]
    pub fn set_gap(&mut self, gap: usize) {
        assert!(gap <= self.len());
        if gap != self.gap() {
            self.move_values(gap);
            self.gap = gap;
        }
    }
    fn move_values(&mut self, gap: usize) {
        let gap_old = self.gap;
        let gap_len = self.gap_len();
        let (src, dest, count) = if gap < gap_old {
            (gap, gap + gap_len, gap_old - gap)
        } else {
            (gap_old + gap_len, gap_old, gap - gap_old)
        };
        let p = self.as_mut_ptr();
        unsafe {
            copy(p.add(src), p.add(dest), count);
        }
    }

    /// Return gap offset of the `GapBuffer<T>`.
    #[inline]
    pub fn gap(&self) -> usize {
        self.gap
    }

    #[inline]
    fn set_gap_with_reserve(&mut self, gap: usize, additional: usize) {
        self.reserve(additional);
        self.set_gap(gap);
    }

    /// Inserts an element at position index within the `GapBuffer<T>`.
    ///
    /// # Panics
    /// Panics if `index > len`.
    ///
    /// Panics if the number of elements in the gap buffer overflows a usize.
    ///
    /// # Computational amount
    /// `O(n)` , `n = |index - self.gap()|`
    #[inline]
    pub fn insert(&mut self, index: usize, element: T) {
        assert!(index <= self.len());
        if self.gap() != index || self.len == self.capacity() {
            self.set_gap_with_reserve(index, 1);
        }
        unsafe {
            write(self.as_mut_ptr().add(index), element);
        }
        self.gap += 1;
        self.len += 1;
    }

    /// Inserts multiple elements at position index within the `GapBuffer<T>`.
    ///
    /// # Panics
    /// Panics if `index > len`.
    ///
    /// Panics if the number of elements in the gap buffer overflows a usize.
    #[inline]
    pub fn insert_iter(&mut self, mut index: usize, iter: impl IntoIterator<Item = T>) {
        assert!(index <= self.len());
        let iter = iter.into_iter();
        let hint = iter.size_hint();
        if let Some(0) = hint.1 {
            return;
        }
        self.set_gap_with_reserve(index, hint.0);
        for value in iter {
            self.insert(index, value);
            index += 1;
        }
    }

    /// Appends an element to the back of a GapBuffer.
    ///
    /// # Panics
    /// Panics if the number of elements in the gap buffer overflows a usize.
    #[inline]
    pub fn push_back(&mut self, value: T) {
        let len = self.len;
        self.insert(len, value);
    }

    /// Prepends an element to the GapBuffer.
    ///
    /// # Panics
    /// Panics if the number of elements in the gap buffer overflows a usize.
    #[inline]
    pub fn push_front(&mut self, value: T) {
        let len = self.len();
        if self.gap() != 0 || len == self.capacity() {
            self.set_gap_with_reserve(0, 1);
        }
        unsafe {
            write(self.as_mut_ptr().add(self.cap - self.len - 1), value);
        }
        self.len += 1;
    }

    /// Removes an element from the GapBuffer and returns it.
    ///
    /// The removed element is replaced by the near the gap.
    ///
    /// # Panics
    /// Panics if `index >= self.len()`.
    ///
    /// # Computational amount
    /// `O(1)`
    ///
    /// # Examples
    /// ```
    /// # #[macro_use] extern crate gapbuf;
    /// # fn main() {
    /// use gapbuf::GapBuffer;
    ///
    /// let mut buf = gap_buffer![1, 2, 3, 4, 5];
    /// buf.set_gap(5);
    /// let value = buf.swap_remove(0);
    /// assert_eq!(value, 1);
    /// assert_eq!(buf, [5, 2, 3, 4]);
    /// # }
    /// ```
    pub fn swap_remove(&mut self, index: usize) -> T {
        assert!(index < self.len());

        unsafe {
            let p = self.as_mut_ptr();
            let value;
            if index < self.gap() {
                let pt = p.add(index);
                value = pt.read();
                self.gap -= 1;
                copy(p.add(self.gap), pt, 1);
                self.len -= 1;
            } else {
                let gap_len = self.gap_len();
                let pt = p.add(index + gap_len);
                value = pt.read();
                copy(p.add(self.gap + gap_len), pt, 1);
                self.len -= 1;
            }
            value
        }
    }

    /// Removes an element from the GapBuffer and returns it.
    ///
    /// # Panics
    /// Panics if `index >= self.len()`.
    ///
    /// # Computational amount
    /// `O(n)`, `n = |index - self.gap()|`
    ///
    /// # Examples
    /// ```
    /// # #[macro_use] extern crate gapbuf;
    /// # fn main() {
    /// use gapbuf::GapBuffer;
    ///
    /// let mut buf = gap_buffer![1, 2, 3, 4, 5];
    /// let value = buf.remove(0);
    /// assert_eq!(value, 1);
    /// assert_eq!(buf, [2, 3, 4, 5]);
    /// # }
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index <= self.len());
        let offset;
        if self.gap() <= index {
            self.set_gap(index);
            offset = self.cap - self.len() + index;
        } else {
            self.set_gap(index + 1);
            self.gap = index;
            offset = index;
        }
        self.len -= 1;
        if self.len == 0 {
            self.gap = 0
        }
        unsafe { ptr::read(self.as_ptr().add(offset)) }
    }

    /// Clears the GapBuffer, removing all values.
    ///
    /// Note that this method has no effect on the allocated capacity of the GapBuffer.
    pub fn clear(&mut self) {
        self.truncate(0);
    }

    /// Shortens the GapBuffer, keeping the first len elements and dropping the rest.
    ///
    /// If len is greater than the GapBuffer's current length, this has no effect.
    ///
    /// Note that this method has no effect on the allocated capacity of the vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate gapbuf; fn main() {
    /// #
    /// let mut buf = gap_buffer![1, 2, 3, 4];
    /// buf.truncate(2);
    /// assert_eq!(buf, [1, 2]);
    /// #
    /// # }
    /// ```
    pub fn truncate(&mut self, len: usize) {
        if needs_drop::<T>() {
            while len < self.len {
                let index = self.len - 1;
                self.remove(index);
            }
        } else {
            if self.gap < len {
                self.set_gap(len);
            } else {
                self.gap = len;
            }
            self.len = len;
        }
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate gapbuf; fn main() {
    /// #
    /// let mut buf = gap_buffer![1, 2, 3, 4];
    /// buf.retain(|&x| x%2 == 0);
    /// assert_eq!(buf, [2, 4]);
    /// #
    /// # }
    /// ```
    pub fn retain(&mut self, mut f: impl FnMut(&T) -> bool) {
        let mut n = 0;
        while n < self.len {
            if f(&self[n]) {
                n += 1;
            } else {
                self.remove(n);
            }
        }
    }

    /// Removes the first element and returns it, or None if the GapBuffer is empty.
    pub fn pop_front(&mut self) -> Option<T> {
        let len = self.len;
        match len {
            0 => None,
            _ => Some(self.remove(0)),
        }
    }

    /// Removes the last element and returns it, or None if the GapBuffer is empty.
    pub fn pop_back(&mut self) -> Option<T> {
        let len = self.len;
        match len {
            0 => None,
            _ => Some(self.remove(len - 1)),
        }
    }

    /// Creates a draining iterator that removes the specified range in the GapBuffer and yields the removed items.
    ///
    /// Note 1: The element range is removed even if the iterator is only partially consumed or not consumed at all.
    /// Note 2: It is unspecified how many elements are removed from the GapBuffer if the Drain value is leaked.
    ///
    /// # Panics
    /// Panics if the `range` is out of GapBuffer.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate gapbuf; fn main() {
    /// #
    /// let mut buf = gap_buffer![1, 2, 3, 4];
    ///
    /// let d : Vec<_> = buf.drain(1..3).collect();
    /// assert_eq!(buf, [1, 4]);
    /// assert_eq!(d, [2, 3]);
    ///
    /// buf.drain(..);
    /// assert_eq!(buf.is_empty(), true);
    /// #
    /// # }
    /// ```
    pub fn drain(&mut self, range: impl RangeBounds<usize>) -> Drain<T> {
        let (idx, len) = self.to_idx_len(range);
        Drain {
            buf: self,
            idx,
            len,
        }
    }
}

impl<T> GapBuffer<T>
where
    T: Clone,
{
    /// Resize the `GapBuffer<T>` in-place so that 'len' is equal to 'new_len'.
    pub fn resize(&mut self, new_len: usize, value: T) {
        let old_len = self.len();
        if new_len < old_len {
            self.truncate(new_len);
        }
        if new_len > old_len {
            self.reserve(new_len - old_len);
            while self.len < new_len {
                self.push_back(value.clone());
            }
        }
    }
}

impl<T> Drop for GapBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            let (s0, s1) = self.as_mut_slices();
            try_finally!(drop_in_place(s0), drop_in_place(s1));
        }
    }
}

impl<T> Deref for GapBuffer<T> {
    type Target = Slice<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &(self.0).0
    }
}
impl<T> DerefMut for GapBuffer<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut (self.0).0
    }
}

impl<T> FromIterator<T> for GapBuffer<T> {
    fn from_iter<S: IntoIterator<Item = T>>(s: S) -> GapBuffer<T> {
        let mut buf = GapBuffer::new();
        buf.extend(s);
        buf
    }
}

impl<T: Clone> Clone for GapBuffer<T> {
    fn clone(&self) -> Self {
        self.iter().cloned().collect()
    }
}
impl<T> Extend<T> for GapBuffer<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let len = self.len;
        self.insert_iter(len, iter);
    }
}
impl<'a, T: 'a + Copy> Extend<&'a T> for GapBuffer<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.extend(iter.into_iter().cloned());
    }
}

#[derive(Hash)]
struct RawGapBuffer<T>(Slice<T>);

impl<T> RawGapBuffer<T> {
    fn new() -> Self {
        RawGapBuffer(Slice::empty())
    }

    fn realloc(&mut self, new_cap: usize) {
        let old_cap = self.0.cap;
        if old_cap == new_cap {
            return;
        }
        if size_of::<T>() != 0 {
            unsafe {
                let old_layout = Self::get_layout(old_cap);
                let new_layout = Self::get_layout(new_cap);
                let p = self.0.ptr.as_ptr() as *mut u8;
                self.0.ptr = if new_cap == 0 {
                    dealloc(p, old_layout);
                    NonNull::dangling()
                } else {
                    NonNull::new(if old_cap == 0 {
                        alloc(new_layout)
                    } else {
                        realloc(p, old_layout, new_layout.size())
                    } as *mut T).unwrap_or_else(|| handle_alloc_error(new_layout))
                };
            }
        }
        self.0.cap = new_cap;
    }
    fn get_layout(cap: usize) -> Layout {
        let new_size = size_of::<T>()
            .checked_mul(cap)
            .expect("memory size overflow");
        Layout::from_size_align(new_size, align_of::<T>()).unwrap()
    }
}
impl<T> Drop for RawGapBuffer<T> {
    fn drop(&mut self) {
        self.realloc(0)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Range, RangeMut
////////////////////////////////////////////////////////////////////////////////

/// Immutable sub-range of [`GapBuffer`]
#[derive(Hash)]
pub struct Range<'a, T: 'a> {
    s: Slice<T>,
    _phantom: PhantomData<&'a [T]>,
}

/// Mutable sub-range of [`GapBuffer`].
#[derive(Hash)]
pub struct RangeMut<'a, T: 'a> {
    s: Slice<T>,
    _phantom: PhantomData<&'a mut [T]>,
}
impl<'a, T: 'a> Range<'a, T> {
    #[inline]
    unsafe fn new(s: Slice<T>) -> Self {
        Range {
            s,
            _phantom: PhantomData::default(),
        }
    }

    /// Construct a new, empty `Range`.
    #[inline]
    pub fn empty() -> Self {
        unsafe { Range::new(Slice::empty()) }
    }
}
impl<'a, T: 'a> RangeMut<'a, T> {
    #[inline]
    unsafe fn new(s: Slice<T>) -> Self {
        RangeMut {
            s,
            _phantom: PhantomData::default(),
        }
    }

    /// Construct a new, empty `RangeMut`.
    #[inline]
    pub fn empty() -> Self {
        unsafe { RangeMut::new(Slice::empty()) }
    }
}

impl<'a, T> Deref for Range<'a, T> {
    type Target = Slice<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.s
    }
}
impl<'a, T> Deref for RangeMut<'a, T> {
    type Target = Slice<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.s
    }
}

impl<'a, T> DerefMut for RangeMut<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.s
    }
}
impl<'a, T> Clone for Range<'a, T> {
    fn clone(&self) -> Self {
        unsafe {
            Range::new(Slice {
                ptr: self.ptr,
                cap: self.cap,
                gap: self.gap,
                len: self.len,
            })
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Slice
////////////////////////////////////////////////////////////////////////////////

/// Sub-range of [`GapBuffer`].
pub struct Slice<T> {
    ptr: NonNull<T>,
    cap: usize,
    gap: usize,
    len: usize,
}
impl<T> Slice<T> {
    /// Construct a new, empty `Slice`.
    pub fn empty() -> Self {
        Slice {
            ptr: NonNull::dangling(),
            cap: 0,
            gap: 0,
            len: 0,
        }
    }

    /// Returns the number of elements in the GapBuffer.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the GapBuffer contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a reference to an element at index or None if out of bounds.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&T> {
        if self.len <= index {
            None
        } else {
            unsafe { Some(&*self.as_ptr().add(self.get_offset(index))) }
        }
    }

    /// Returns a mutable reference to an element at index or None if out of bounds.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if self.len <= index {
            None
        } else {
            let p = self.as_mut_ptr();
            let o = self.get_offset(index);
            unsafe { Some(&mut *p.add(o)) }
        }
    }

    /// Swaps two elements in the GapBuffer.
    ///
    /// # Arguments
    ///
    /// * a - The index of the first element
    /// * b - The index of the second element
    ///
    /// # Panics
    /// Panics if `a >= self.len()` or `b >= self.len()`.
    #[inline]
    pub fn swap(&mut self, a: usize, b: usize) {
        let oa = self.get_offset(a);
        let ob = self.get_offset(b);
        let p = self.as_mut_ptr();
        unsafe { ptr::swap(p.add(oa), p.add(ob)) }
    }

    pub fn range(&self, range: impl RangeBounds<usize>) -> Range<T> {
        unsafe { Range::new(self.range_slice(range)) }
    }
    pub fn range_mut(&mut self, range: impl RangeBounds<usize>) -> RangeMut<T> {
        unsafe { RangeMut::new(self.range_slice(range)) }
    }
    unsafe fn range_slice(&self, range: impl RangeBounds<usize>) -> Slice<T> {
        let (idx, len) = self.to_idx_len(range);
        let (gap, gap_remove) = if idx < self.gap {
            (self.gap - idx, 0)
        } else {
            (0, self.gap_len())
        };

        Slice {
            ptr: NonNull::new(self.ptr.as_ptr().add(idx + gap_remove)).unwrap(),
            cap: self.cap - (self.len - len + gap_remove),
            gap,
            len,
        }
    }
    fn to_idx_len(&self, range: impl RangeBounds<usize>) -> (usize, usize) {
        use std::ops::Bound::*;
        const MAX: usize = usize::max_value();
        let len = self.len;
        let idx = match range.start_bound() {
            Included(&idx) => idx,
            Excluded(&MAX) => panic!("attempted to index slice up to maximum usize"),
            Excluded(&idx) => idx + 1,
            Unbounded => 0,
        };
        if idx > len {
            panic!("index {} out of range for slice of length {}", idx, len);
        }

        let end = match range.end_bound() {
            Included(&MAX) => panic!("attempted to index slice up to maximum usize"),
            Included(&idx) => idx + 1,
            Excluded(&idx) => idx,
            Unbounded => len,
        };
        if end > len {
            panic!("index {} out of range for slice of length {}", end, len);
        }

        if end < idx {
            panic!("slice index starts at {} but ends at {}", idx, end);
        }
        (idx, end - idx)
    }

    pub fn as_slices(&self) -> (&[T], &[T]) {
        unsafe {
            let p0 = self.as_ptr();
            let c1 = self.len - self.gap;
            let p1 = p0.add(self.cap - c1);
            (
                slice::from_raw_parts(p0, self.gap),
                slice::from_raw_parts(p1, c1),
            )
        }
    }
    pub fn as_mut_slices(&mut self) -> (&mut [T], &mut [T]) {
        unsafe {
            let p0 = self.as_mut_ptr();
            let c1 = self.len - self.gap;
            let p1 = p0.add(self.cap - c1);
            (
                slice::from_raw_parts_mut(p0, self.gap),
                slice::from_raw_parts_mut(p1, c1),
            )
        }
    }

    pub fn iter(&self) -> Iter<T> {
        Iter { s: self, idx: 0 }
    }
    pub fn iter_mut(&mut self) -> IterMut<T> {
        IterMut { s: self, idx: 0 }
    }

    #[inline]
    fn get_offset(&self, index: usize) -> usize {
        assert!(index < self.len);
        index + if index < self.gap { 0 } else { self.gap_len() }
    }

    #[inline]
    fn gap_len(&self) -> usize {
        self.cap - self.len
    }

    fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }
    fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }
}
unsafe impl<T: Sync> Sync for Slice<T> {}
unsafe impl<T: Send> Send for Slice<T> {}

////////////////////////////////////////////////////////////////////////////////
// Default
////////////////////////////////////////////////////////////////////////////////

impl<T> Default for GapBuffer<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
impl<'a, T> Default for Range<'a, T> {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}
impl<'a, T> Default for RangeMut<'a, T> {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Index, IndexMut
////////////////////////////////////////////////////////////////////////////////

impl<T> Index<usize> for GapBuffer<T> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &T {
        self.deref().index(index)
    }
}
impl<T> IndexMut<usize> for GapBuffer<T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut T {
        self.deref_mut().index_mut(index)
    }
}

impl<'a, T> Index<usize> for Range<'a, T> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &T {
        self.deref().index(index)
    }
}
impl<'a, T> Index<usize> for RangeMut<'a, T> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &T {
        self.deref().index(index)
    }
}
impl<'a, T> IndexMut<usize> for RangeMut<'a, T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut T {
        self.deref_mut().index_mut(index)
    }
}

impl<T> Index<usize> for Slice<T> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &T {
        self.get(index).expect("index out of bounds")
    }
}
impl<T> IndexMut<usize> for Slice<T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut T {
        self.get_mut(index).expect("index out of bounds")
    }
}

////////////////////////////////////////////////////////////////////////////////
// Debug
////////////////////////////////////////////////////////////////////////////////

impl<T> Debug for GapBuffer<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.deref().fmt(f)
    }
}
impl<'a, T> Debug for Range<'a, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.deref().fmt(f)
    }
}
impl<'a, T> Debug for RangeMut<'a, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.deref().fmt(f)
    }
}
impl<T> Debug for Slice<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.debug_list().entries(self).finish()
    }
}

impl<T: Hash> Hash for Slice<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for value in self {
            value.hash(state);
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Eq, PartialEq
////////////////////////////////////////////////////////////////////////////////

impl<T, S> PartialEq<S> for GapBuffer<T>
where
    T: PartialEq,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn eq(&self, other: &S) -> bool {
        self.deref().eq(other)
    }
}
impl<T: Eq> Eq for GapBuffer<T> {}

impl<'a, T, S> PartialEq<S> for Range<'a, T>
where
    T: PartialEq,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn eq(&self, other: &S) -> bool {
        self.deref().eq(other)
    }
}
impl<'a, T: Eq> Eq for Range<'a, T> {}

impl<'a, T, S> PartialEq<S> for RangeMut<'a, T>
where
    T: PartialEq,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn eq(&self, other: &S) -> bool {
        self.deref().eq(other)
    }
}
impl<'a, T: Eq> Eq for RangeMut<'a, T> {}

impl<T, S> PartialEq<S> for Slice<T>
where
    T: PartialEq,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn eq(&self, other: &S) -> bool {
        self.iter().eq(other)
    }
}
impl<T: Eq> Eq for Slice<T> {}

////////////////////////////////////////////////////////////////////////////////
// Ord, PartialOrd
////////////////////////////////////////////////////////////////////////////////

impl<T, S> PartialOrd<S> for GapBuffer<T>
where
    T: PartialOrd,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn partial_cmp(&self, other: &S) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<T: Ord> Ord for GapBuffer<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}

impl<'a, T, S> PartialOrd<S> for Range<'a, T>
where
    T: PartialOrd,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn partial_cmp(&self, other: &S) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<'a, T: Ord> Ord for Range<'a, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}

impl<'a, T, S> PartialOrd<S> for RangeMut<'a, T>
where
    T: PartialOrd,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn partial_cmp(&self, other: &S) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<'a, T: Ord> Ord for RangeMut<'a, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}

impl<T, S> PartialOrd<S> for Slice<T>
where
    T: PartialOrd,
    S: ?Sized,
    for<'b> &'b S: IntoIterator<Item = &'b T>,
{
    fn partial_cmp(&self, other: &S) -> Option<Ordering> {
        self.iter().partial_cmp(other)
    }
}

impl<T: Ord> Ord for Slice<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other)
    }
}

////////////////////////////////////////////////////////////////////////////////
// iterator
////////////////////////////////////////////////////////////////////////////////

pub struct Iter<'a, T: 'a> {
    s: &'a Slice<T>,
    idx: usize,
}
impl<'a, T: 'a> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.idx == self.s.len {
            None
        } else {
            let i = self.idx;
            self.idx += 1;
            Some(&self.s[i])
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.s.len - self.idx;
        (len, Some(len))
    }
}
impl<'a, T: 'a> ExactSizeIterator for Iter<'a, T> {}
impl<'a, T: 'a> FusedIterator for Iter<'a, T> {}

pub struct IterMut<'a, T: 'a> {
    s: &'a mut Slice<T>,
    idx: usize,
}
impl<'a, T: 'a> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        if self.idx == self.s.len {
            None
        } else {
            let p = self.s.as_mut_ptr();
            let o = self.s.get_offset(self.idx);
            self.idx += 1;
            unsafe { Some(&mut *p.add(o)) }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.s.len - self.idx;
        (len, Some(len))
    }
}
impl<'a, T: 'a> ExactSizeIterator for IterMut<'a, T> {}
impl<'a, T: 'a> FusedIterator for IterMut<'a, T> {}

pub struct IntoIter<T> {
    buf: GapBuffer<T>,
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.buf.pop_front()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.buf.len();
        (len, Some(len))
    }
}
impl<T> ExactSizeIterator for IntoIter<T> {}
impl<T> FusedIterator for IntoIter<T> {}

impl<T> IntoIterator for GapBuffer<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter { buf: self }
    }
}

impl<'a, T> IntoIterator for &'a GapBuffer<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
impl<'a, 'b, T> IntoIterator for &'a Range<'b, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
impl<'a, 'b, T> IntoIterator for &'a RangeMut<'b, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a Slice<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut GapBuffer<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}
impl<'a, T> IntoIterator for &'a mut RangeMut<'a, T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}
impl<'a, T> IntoIterator for &'a mut Slice<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}

pub struct Drain<'a, T: 'a> {
    buf: &'a mut GapBuffer<T>,
    idx: usize,
    len: usize,
}
impl<'a, T> Iterator for Drain<'a, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            Some(self.buf.remove(self.idx))
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}
impl<'a, T> Drop for Drain<'a, T> {
    fn drop(&mut self) {
        let len = self.len;
        self.nth(len);
    }
}

impl<'a, T> ExactSizeIterator for Drain<'a, T> {}
impl<'a, T> FusedIterator for Drain<'a, T> {}
