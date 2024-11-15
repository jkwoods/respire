//! Generic ring-related traits.
use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// A generic element of a ring (with `1`), supporting the operations `+=`, `-=`, `*=` and the
/// special elements `0` and `1`.
///
/// The `From<u64>` trait is expected to be implemented as the canonical map `Z -> R`. That is, it
/// ought to be equal to `1 + ... + 1` for the appropriate number of `1`s.
///
/// Non-inplace arithmetic is intentionally not part of this trait. In general, a `RingElement`
/// need not be `Clone`. To work around this, see [`RingElementRef`].
pub trait RingElement:
    Sized
    + Clone
    + PartialEq
    + Eq
    + Sync
    + From<u64>
    + for<'a> AddAssign<&'a Self>
    + for<'a> SubAssign<&'a Self>
    + for<'a> MulAssign<&'a Self>
where
    for<'a> &'a Self: RingElementRef<Self>,
{
    /// Constructs the zero element (additive identity) of the ring.
    fn zero() -> Self;
    /// Constructs the one element (multiplicative identity) of the ring.
    fn one() -> Self;

    /// Add `a * b` to `self` in-place. This method is used for matrix multiplication, so optimizing
    /// it may be desirable.
    fn add_eq_mul(&mut self, a: &Self, b: &Self) {
        *self += &(a * b);
    }
}

/// A reference to a RingElement that supports non-inplace ring operations. This is required for
/// e.g. matrices over a ring to avoid possibly expensive copying.
pub trait RingElementRef<Owned: RingElement>:
    Sized
    + Clone
    + Sync
    + Add<Self, Output = Owned>
    + Sub<Self, Output = Owned>
    + Mul<Self, Output = Owned>
    + Neg<Output = Owned>
where
    for<'a> &'a Owned: RingElementRef<Owned>,
{
}

pub trait NormedRingElement: RingElement
where
    for<'a> &'a Self: RingElementRef<Self>,
{
    fn norm(&self) -> u64;
}

///
/// # Safety
///
/// `R` is `RingCompatible<S>` implies that the underlying memory representation of `R` is compatible
/// with that of `S`. This is typically enforced by using `repr(u64)` for `IntMod` and `repr(C)` for
/// the various types containing `IntMod`s. (For example, `IntMod<7>` is `RingCompatible<IntMod<25>>`,
/// since they are both represented by a transparent `u64`.)
///
pub unsafe trait RingCompatible<Other: RingElement>: RingElement
where
    for<'a> &'a Self: RingElementRef<Self>,
    for<'a> &'a Other: RingElementRef<Other>,
{
    fn convert(self) -> Other {
        let ptr = &self as *const Self as *const Other;
        // Safety: Self is RingCompatible<Other> implies this conversion is safe
        let val = unsafe { ptr.read() };
        std::mem::forget(self);
        val
    }

    fn convert_ref(&self) -> &Other {
        unsafe { &*(self as *const Self as *const Other) }
    }
}
