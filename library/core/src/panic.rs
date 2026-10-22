//! Where panics happen.

use crate::fmt::{self, Display, Formatter};

/// A place in the source: a file, a line and a column, each counted
/// from one, as `#[track_caller]` functions get their caller's.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[lang = "Location"]
pub struct Location<'a> {
    file: &'a str,
    line: u32,
    col: u32,
}

impl<'a> Location<'a> {
    /// Where the function calling this was called, through
    /// `#[track_caller]` functions; where this is called otherwise.
    #[track_caller]
    pub fn caller() -> &'static Location<'static> {
        crate::intrinsics::caller_location()
    }

    /// The file's path, as the compiler was given it.
    pub const fn file(&self) -> &'a str {
        self.file
    }

    pub const fn line(&self) -> u32 {
        self.line
    }

    pub const fn column(&self) -> u32 {
        self.col
    }
}

/// `file:line:column`
impl Display for Location<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.file)?;
        f.write_str(":")?;
        Display::fmt(&self.line, f)?;
        f.write_str(":")?;
        Display::fmt(&self.col, f)
    }
}
