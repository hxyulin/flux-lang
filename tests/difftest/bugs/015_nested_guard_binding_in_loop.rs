//! Wrong rejection: a binding introduced by a match inside a match
//! guard is considered initialized on the loop's next iteration.
//!
//! rustc accepts this and prints `no`, `yes`, `yes`. flux reports
//! "cannot assign twice to immutable variable `binding`" in both modes.
//! Removing the loop makes flux accept the program.
//!
//! Found by difftest seed 109252 with default weights after reenabling
//! parenthesized patterns and bare macros. Manually minimized; this is
//! an initialization-checking issue, separate from the parser repairs.

fn main() {
    for value in [0u8, 1, 2] {
        match value {
            1 | 2 if match 5u8 { binding => binding == 5 } => println!("yes"),
            _ => println!("no"),
        }
    }
}
