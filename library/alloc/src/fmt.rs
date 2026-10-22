//! `core::fmt`, and `format`, which `format!` calls.

pub use core::fmt::*;

use crate::string::String;

/// The text `args` makes, with the room rustc's makes first.
pub fn format(args: Arguments<'_>) -> String {
    match args.as_str() {
        Some(s) => String::from(s),
        None => {
            let mut output = String::with_capacity(args.estimated_capacity());
            output
                .write_fmt(args)
                .expect("a formatting trait implementation returned an error when the underlying stream did not");
            output
        }
    }
}
