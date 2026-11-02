//! Explicit-consumption guidance, not compiler-enforced linear types.
//!
//! A live marker panics when dropped outside an existing panic. During
//! unwinding it does nothing. Leaks, aborts and process termination bypass it.
//! This crate never uses destructor execution as a memory-safety invariant.
//!
//! ```
//! use phantomlinear::{Linear, Cleanup};
//! let value = Linear::new(String::from("resource"));
//! let guard = Cleanup::new(value, |value| drop(value.into_inner()));
//! drop(guard); // Invokes the explicitly supplied cleanup action.
//! ```
//!
//! Ownership still prevents duplicate consumption:
//! ```compile_fail
//! use phantomlinear::Linear;
//! let value = Linear::new(42);
//! let _ = value.into_inner();
//! let _ = value.into_inner(); // moved
//! ```
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::ops::{Deref, DerefMut};

/// Runtime obligation to explicitly complete a resource's protocol.
///
/// Deliberately neither `Copy` nor `Clone`. Not zero-sized: the armed bit is
/// necessary to distinguish completion from accidental destruction.
/// `#[must_use]` is advisory and does not propagate to enclosing structures.
///
/// Never use this marker to justify unsafe code that requires cleanup to run.
#[must_use = "explicitly complete the resource's protocol"]
#[derive(Debug)]
pub struct PhantomLinear {
    armed: bool,
    name: &'static str,
}

impl PhantomLinear {
    /// Creates an armed marker with a default diagnostic name.
    pub const fn new() -> Self {
        Self::named("linear resource")
    }

    /// Creates an armed marker with a static diagnostic name.
    pub const fn named(name: &'static str) -> Self {
        Self { armed: true, name }
    }

    /// Discharges this obligation. Call only after choosing a valid terminal
    /// operation, or when transferring responsibility to another owner.
    ///
    /// This is intentionally a safe protocol operation, not a safety promise.
    pub fn complete(mut self) {
        self.armed = false;
    }
}

impl Default for PhantomLinear {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PhantomLinear {
    fn drop(&mut self) {
        if self.armed && !std::thread::panicking() {
            panic!(
                "PhantomLinear: `{}` dropped without explicit completion",
                self.name
            );
        }
    }
}

/// Owning wrapper requiring an explicit ownership-transfer operation.
///
/// Extracting `T` transfers the obligation to the caller; it does not prove
/// that the caller actually finalized the underlying resource. Borrowing does
/// not discharge the obligation. Does not implement `Deref`, avoiding implicit
/// access to arbitrary resource APIs.
#[must_use = "consume this value explicitly or install a cleanup guard"]
#[derive(Debug)]
pub struct Linear<T> {
    value: T,
    marker: PhantomLinear,
}

impl<T> Linear<T> {
    /// Wraps a value with an armed obligation.
    pub fn new(value: T) -> Self {
        Self::named(value, std::any::type_name::<T>())
    }

    /// Wraps a value with a custom diagnostic name.
    pub fn named(value: T, name: &'static str) -> Self {
        Self {
            value,
            marker: PhantomLinear::named(name),
        }
    }

    /// Borrows the wrapped value without completing the obligation.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Mutably borrows the value without completing the obligation.
    ///
    /// `mem::replace` can move the original resource out; consequently this is
    /// not an API for enforcing a domain-specific finalization protocol.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Transfers the value out, discharging the wrapper's obligation.
    pub fn into_inner(self) -> T {
        let Self { value, marker } = self;
        marker.complete();
        value
    }

    /// Runs an explicit terminal operation on the owned value.
    ///
    /// The marker is disarmed before entering user code. A panic from `f`
    /// therefore propagates without an additional marker panic.
    pub fn consume<R>(self, f: impl FnOnce(T) -> R) -> R {
        f(self.into_inner())
    }

    /// Transfers the obligation to a newly wrapped result.
    /// A panic in `f` propagates normally and no new obligation is created.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Linear<U> {
        Linear::new(f(self.into_inner()))
    }

    /// Installs a consuming fallback, including on early return and unwind.
    pub fn with_cleanup<F: FnOnce(T)>(self, f: F) -> Cleanup<T, F> {
        Cleanup::new(self.into_inner(), f)
    }
}

impl<T> AsRef<T> for Linear<T> {
    fn as_ref(&self) -> &T {
        self.get()
    }
}

impl<T> AsMut<T> for Linear<T> {
    fn as_mut(&mut self) -> &mut T {
        self.get_mut()
    }
}

/// A domain-defined, consuming finalization action.
///
/// Implementations should not panic, especially when called from a destructor.
/// Fallible finalization should be performed explicitly before scope exit.
pub trait Consume {
    /// Finalizes this owned value.
    fn consume(self);
}

/// Affine adapter: runs `Consume::consume` on ordinary drop and unwind.
///
/// Uses `Option` rather than unsafe `ManuallyDrop` extraction. Extraction
/// removes the value before the wrapper's destructor can run.
#[derive(Debug)]
pub struct ReScopeDrop<T: Consume> {
    value: Option<T>,
}

impl<T: Consume> ReScopeDrop<T> {
    /// Creates a scope-cleaned owner.
    pub fn new(value: T) -> Self {
        Self { value: Some(value) }
    }

    /// Removes the value without running its consuming fallback.
    pub fn into_inner(mut self) -> T {
        self.value.take().expect("ReScopeDrop invariant")
    }

    /// Runs consuming cleanup now instead of at scope exit.
    pub fn consume(self) {
        drop(self);
    }
}

impl<T: Consume> Deref for ReScopeDrop<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().expect("ReScopeDrop invariant")
    }
}

impl<T: Consume> DerefMut for ReScopeDrop<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("ReScopeDrop invariant")
    }
}

impl<T: Consume> AsRef<T> for ReScopeDrop<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: Consume> AsMut<T> for ReScopeDrop<T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T: Consume> Drop for ReScopeDrop<T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            value.consume();
        }
    }
}

/// Cleanup guard accepting a once-only consuming closure.
///
/// Unlike `PhantomLinear`, this runs its callback during panic unwinding.
/// A callback panic during unwinding may abort the process. Callbacks must
/// therefore be unwind-safe in the application-specific sense; `UnwindSafe`
/// bounds would not prove that they cannot panic.
///
/// Leaking the guard bypasses cleanup, as with all Rust destructors.
pub struct Cleanup<T, F: FnOnce(T)> {
    state: Option<(T, F)>,
}

impl<T, F: FnOnce(T)> Cleanup<T, F> {
    /// Installs a fallback that takes ownership of the guarded value.
    pub fn new(value: T, cleanup: F) -> Self {
        Self {
            state: Some((value, cleanup)),
        }
    }

    /// Extracts the value and disables the fallback.
    /// Captured values inside the unused callback are dropped normally.
    pub fn into_inner(mut self) -> T {
        let (value, cleanup) = self.state.take().expect("Cleanup invariant");
        drop(cleanup);
        value
    }

    /// Runs the fallback immediately.
    pub fn run(self) {
        drop(self);
    }
}

impl<T, F: FnOnce(T)> Deref for Cleanup<T, F> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.state.as_ref().expect("Cleanup invariant").0
    }
}

impl<T, F: FnOnce(T)> DerefMut for Cleanup<T, F> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.state.as_mut().expect("Cleanup invariant").0
    }
}

impl<T, F: FnOnce(T)> AsRef<T> for Cleanup<T, F> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T, F: FnOnce(T)> AsMut<T> for Cleanup<T, F> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T, F: FnOnce(T)> Drop for Cleanup<T, F> {
    fn drop(&mut self) {
        if let Some((value, cleanup)) = self.state.take() {
            cleanup(value);
        }
    }
}

/// Creates a [`Cleanup`] guard; each argument is evaluated exactly once.
///
/// ```
/// let guard = phantomlinear::cleanup!(String::from("temporary"), |s| drop(s));
/// guard.run();
/// ```
#[macro_export]
macro_rules! cleanup {
    ($value:expr, $cleanup:expr $(,)?) => {
        $crate::Cleanup::new($value, $cleanup)
    };
}
