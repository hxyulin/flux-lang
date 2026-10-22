//! `ControlFlow`, and the `Try` and `FromResidual` traits that
//! `Iterator::try_fold` and its relatives are written with, as in rustc.
//! Stable Rust cannot name the traits (they are unstable), only use them
//! through `?` and the methods bounded by them; `?` itself is built into
//! the compiler for `Option`, `Result` and `ControlFlow`, the types that
//! implement them.

use crate::convert::{From, Infallible};
use crate::option::Option::{self, None, Some};
use crate::result::Result::{self, Err, Ok};

/// Whether an operation should stop early (`Break`) or go on as usual
/// (`Continue`). `?` on a `ControlFlow` gives the `Continue` value, or
/// returns the `Break` from a function that returns a `ControlFlow`.
#[lang = "ControlFlow"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFlow<B, C = ()> {
    /// Go on to the next step, with this value.
    Continue(C),
    /// Stop now, with this value.
    Break(B),
}

use ControlFlow::{Break, Continue};

impl<B, C> ControlFlow<B, C> {
    /// Whether it is `Break`.
    pub fn is_break(&self) -> bool {
        match self {
            Break(_) => true,
            Continue(_) => false,
        }
    }

    /// Whether it is `Continue`.
    pub fn is_continue(&self) -> bool {
        match self {
            Break(_) => false,
            Continue(_) => true,
        }
    }

    /// The `Break` value, if it is one.
    pub fn break_value(self) -> Option<B> {
        match self {
            Continue(..) => None,
            Break(x) => Some(x),
        }
    }

    /// The `Continue` value, if it is one.
    pub fn continue_value(self) -> Option<C> {
        match self {
            Continue(x) => Some(x),
            Break(..) => None,
        }
    }

    /// `f` of the `Break` value.
    pub fn map_break<T, F: FnOnce(B) -> T>(self, f: F) -> ControlFlow<T, C> {
        match self {
            Continue(x) => Continue(x),
            Break(x) => Break(f(x)),
        }
    }

    /// `f` of the `Continue` value.
    pub fn map_continue<T, F: FnOnce(C) -> T>(self, f: F) -> ControlFlow<B, T> {
        match self {
            Continue(x) => Continue(f(x)),
            Break(x) => Break(x),
        }
    }
}

use crate::ops::FnOnce;

/// A type `?` works on: a value to go on with (`Output`), or what to
/// return early (`Residual`), which `FromResidual` turns back into a
/// value of the type returned.
pub trait Try: FromResidual<<Self as Try>::Residual> {
    /// The value `?` gives when it goes on.
    type Output;
    /// What `?` returns early with, which keeps only the stopping case.
    type Residual;

    /// The value that goes on with `output`.
    fn from_output(output: Self::Output) -> Self;

    /// Whether to go on, with the output, or stop, with the residual.
    fn branch(self) -> ControlFlow<Self::Residual, Self::Output>;
}

/// A type an early return of `R` (another type's residual) makes.
pub trait FromResidual<R> {
    fn from_residual(residual: R) -> Self;
}

impl<B, C> Try for ControlFlow<B, C> {
    type Output = C;
    type Residual = ControlFlow<B, Infallible>;

    fn from_output(output: C) -> Self {
        Continue(output)
    }

    fn branch(self) -> ControlFlow<ControlFlow<B, Infallible>, C> {
        match self {
            Continue(c) => Continue(c),
            Break(b) => Break(Break(b)),
        }
    }
}

impl<B, C> FromResidual<ControlFlow<B, Infallible>> for ControlFlow<B, C> {
    fn from_residual(residual: ControlFlow<B, Infallible>) -> Self {
        match residual {
            Break(b) => Break(b),
            Continue(never) => match never {},
        }
    }
}

impl<T> Try for Option<T> {
    type Output = T;
    type Residual = Option<Infallible>;

    fn from_output(output: T) -> Self {
        Some(output)
    }

    fn branch(self) -> ControlFlow<Option<Infallible>, T> {
        match self {
            Some(v) => Continue(v),
            None => Break(None),
        }
    }
}

impl<T> FromResidual<Option<Infallible>> for Option<T> {
    fn from_residual(residual: Option<Infallible>) -> Self {
        match residual {
            None => None,
            Some(never) => match never {},
        }
    }
}

impl<T, E> Try for Result<T, E> {
    type Output = T;
    type Residual = Result<Infallible, E>;

    fn from_output(output: T) -> Self {
        Ok(output)
    }

    fn branch(self) -> ControlFlow<Result<Infallible, E>, T> {
        match self {
            Ok(v) => Continue(v),
            Err(e) => Break(Err(e)),
        }
    }
}

impl<T, E, F: From<E>> FromResidual<Result<Infallible, E>> for Result<T, F> {
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Err(e) => Err(From::from(e)),
            Ok(never) => match never {},
        }
    }
}
