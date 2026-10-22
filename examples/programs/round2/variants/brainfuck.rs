//! flux variant of ../brainfuck.rs, minimally rewritten to compile with flux:
//! core::iter::empty() becomes None.into_iter(); the try_fold digit
//! parsing becomes loops.
//!
//! A Brainfuck compiler and interpreter: source is compiled to a compact
//! instruction enum with run-length folding (`+++` becomes `Add(3)`),
//! `[-]` recognized as `Clear`, and jumps resolved with a fixed-size
//! bracket stack, reporting unbalanced brackets as errors. The VM runs on a
//! 30 000-cell wrapping tape, writing output into a buffer through an
//! `Output` trait, with a step budget. Runs hello world, a program reading
//! its input, a squares printer, and (as a benchmark) a long-running nested
//! loop program.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add(u8),
    Move(i32),
    Out,
    In,
    Clear,
    /// Jump past the matching `JumpBack` if the cell is zero.
    JumpIfZero(u32),
    /// Jump back after the matching `JumpIfZero` if the cell is nonzero.
    JumpBack(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompileError {
    UnmatchedOpen { at: usize },
    UnmatchedClose { at: usize },
    TooLong,
    TooDeep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunError {
    TapeUnderflow { pc: usize },
    TapeOverflow { pc: usize },
    StepLimit,
    OutputFull,
}

const MAX_OPS: usize = 4096;
const MAX_DEPTH: usize = 64;
const TAPE: usize = 30_000;

struct Program {
    ops: [Op; MAX_OPS],
    len: usize,
}

impl Program {
    fn compile(src: &[u8]) -> Result<Program, CompileError> {
        let mut prog = Program { ops: [Op::Clear; MAX_OPS], len: 0 };
        let mut stack = [(0usize, 0usize); MAX_DEPTH];
        let mut depth = 0;
        let mut i = 0;
        while i < src.len() {
            let c = src[i];
            let op = match c {
                b'+' | b'-' => {
                    let mut n: u8 = 0;
                    while i < src.len() && matches!(src[i], b'+' | b'-') {
                        n = if src[i] == b'+' { n.wrapping_add(1) } else { n.wrapping_sub(1) };
                        i += 1;
                    }
                    i -= 1;
                    Some(Op::Add(n))
                }
                b'>' | b'<' => {
                    let mut n = 0i32;
                    while i < src.len() && matches!(src[i], b'>' | b'<') {
                        n += if src[i] == b'>' { 1 } else { -1 };
                        i += 1;
                    }
                    i -= 1;
                    Some(Op::Move(n))
                }
                b'.' => Some(Op::Out),
                b',' => Some(Op::In),
                b'[' if src[i..].starts_with(b"[-]") => {
                    i += 2;
                    Some(Op::Clear)
                }
                b'[' => {
                    if depth == MAX_DEPTH {
                        return Err(CompileError::TooDeep);
                    }
                    stack[depth] = (prog.len, i);
                    depth += 1;
                    Some(Op::JumpIfZero(0))
                }
                b']' => {
                    if depth == 0 {
                        return Err(CompileError::UnmatchedClose { at: i });
                    }
                    depth -= 1;
                    let (open, _) = stack[depth];
                    prog.ops[open] = Op::JumpIfZero(prog.len as u32);
                    Some(Op::JumpBack(open as u32))
                }
                _ => None,
            };
            if let Some(op) = op {
                if prog.len == MAX_OPS {
                    return Err(CompileError::TooLong);
                }
                prog.ops[prog.len] = op;
                prog.len += 1;
            }
            i += 1;
        }
        if depth > 0 {
            return Err(CompileError::UnmatchedOpen { at: stack[depth - 1].1 });
        }
        Ok(prog)
    }

    fn ops(&self) -> &[Op] {
        &self.ops[..self.len]
    }
}

trait Output {
    fn put(&mut self, byte: u8) -> Result<(), RunError>;
}

struct Buffer<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Buffer<N> {
    fn new() -> Self {
        Buffer { bytes: [0; N], len: 0 }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("<invalid utf-8>")
    }
}

impl<const N: usize> Output for Buffer<N> {
    fn put(&mut self, byte: u8) -> Result<(), RunError> {
        let slot = self.bytes.get_mut(self.len).ok_or(RunError::OutputFull)?;
        *slot = byte;
        self.len += 1;
        Ok(())
    }
}

/// Counts and checksums output instead of storing it.
#[derive(Default)]
struct Checksum {
    count: u64,
    hash: u32,
}

impl Output for Checksum {
    fn put(&mut self, byte: u8) -> Result<(), RunError> {
        self.count += 1;
        self.hash = self.hash.rotate_left(5) ^ byte as u32;
        Ok(())
    }
}

struct Machine {
    tape: [u8; TAPE],
    ptr: usize,
    steps: u64,
}

impl Machine {
    fn new() -> Self {
        Machine { tape: [0; TAPE], ptr: 0, steps: 0 }
    }

    fn run<O: Output, I: Iterator<Item = u8>>(
        &mut self,
        prog: &Program,
        input: &mut I,
        out: &mut O,
        budget: u64,
    ) -> Result<u64, RunError> {
        let ops = prog.ops();
        let mut pc = 0;
        while pc < ops.len() {
            self.steps += 1;
            if self.steps > budget {
                return Err(RunError::StepLimit);
            }
            match ops[pc] {
                Op::Add(n) => self.tape[self.ptr] = self.tape[self.ptr].wrapping_add(n),
                Op::Move(d) => {
                    let p = self.ptr as i64 + d as i64;
                    if p < 0 {
                        return Err(RunError::TapeUnderflow { pc });
                    }
                    if p >= TAPE as i64 {
                        return Err(RunError::TapeOverflow { pc });
                    }
                    self.ptr = p as usize;
                }
                Op::Out => out.put(self.tape[self.ptr])?,
                Op::In => self.tape[self.ptr] = input.next().unwrap_or(0),
                Op::Clear => self.tape[self.ptr] = 0,
                Op::JumpIfZero(target) => {
                    if self.tape[self.ptr] == 0 {
                        pc = target as usize;
                    }
                }
                Op::JumpBack(target) => {
                    if self.tape[self.ptr] != 0 {
                        pc = target as usize;
                    }
                }
            }
            pc += 1;
        }
        Ok(self.steps)
    }
}

const HELLO: &[u8] = b"++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";

/// Reverses its input (up to a NUL).
const REVERSE: &[u8] = b">,[>,]<[.<]";

/// Prints the squares 0..=10000, from Daniel Cristofani's collection.
const SQUARES: &[u8] = b"++++[>+++++<-]>[<+++++>-]+<+[>[>+>+<<-]++>>[<<+>>-]>>>[-]++>[-]+>>>+[[-]++++++>>>]<<<[[<++++++++<++>>-]+<.<[>----<-]<]<<[>>>>>[>>>[-]+++++++++<[>-<-]+++++++++>[-[<->-]+[<<<]]<[>+<-]>]<<-]<<-]";

/// Three nested 255-iteration loops: about 116 million steps.
const BUSY: &[u8] = b"-[>-[>-[>+>+<<-]<-]<-]>>>.";

fn describe(e: CompileError) -> (&'static str, usize) {
    match e {
        CompileError::UnmatchedOpen { at } => ("unmatched [", at),
        CompileError::UnmatchedClose { at } => ("unmatched ]", at),
        CompileError::TooLong => ("too long", 0),
        CompileError::TooDeep => ("too deep", 0),
    }
}

fn print_str(s: &str) {
    unsafe { printf(c"%.*s".as_ptr(), s.len() as i32, s.as_ptr()) };
}

fn main() {
    let hello = Program::compile(HELLO).unwrap();
    let mut out: Buffer<64> = Buffer::new();
    let steps = Machine::new().run(&hello, &mut None.into_iter(), &mut out, 1_000_000).unwrap();
    unsafe { printf(c"hello: %zu ops (from %zu chars), %llu steps: ".as_ptr(), hello.len, HELLO.len(), steps) };
    print_str(out.as_str());

    let rev = Program::compile(REVERSE).unwrap();
    let mut out: Buffer<64> = Buffer::new();
    let mut input = b"stressed".iter().copied();
    Machine::new().run(&rev, &mut input, &mut out, 10_000).unwrap();
    print_str("reverse: ");
    print_str(out.as_str());
    print_str("\n");

    let squares = Program::compile(SQUARES).unwrap();
    let mut out: Buffer<1024> = Buffer::new();
    let mut m = Machine::new();
    let steps = m.run(&squares, &mut None.into_iter(), &mut out, 10_000_000).unwrap();
    let text = out.as_str();
    let count = text.lines().count();
    let last = text.lines().last().unwrap_or("");
    let mut sum = 0u64;
    for l in text.lines() {
        let mut acc = 0u64;
        for b in l.bytes() {
            acc = acc * 10 + (b - b'0') as u64;
        }
        sum += acc;
    }
    unsafe { printf(c"squares: %zu lines, last ".as_ptr(), count) };
    print_str(last);
    unsafe { printf(c", sum %llu, %llu steps\n".as_ptr(), sum, steps) };

    // A tiny output buffer overflows; a budget stops a loop that never ends.
    let mut tiny: Buffer<4> = Buffer::new();
    let r = Machine::new().run(&hello, &mut None.into_iter(), &mut tiny, 1_000_000);
    unsafe { printf(c"tiny buffer: %d, kept '".as_ptr(), (r == Err(RunError::OutputFull)) as i32) };
    print_str(tiny.as_str());
    print_str("'\n");
    let forever = Program::compile(b"+[]").unwrap();
    let r = Machine::new().run(&forever, &mut None.into_iter(), &mut Checksum::default(), 5000);
    unsafe { printf(c"infinite loop stopped: %d\n".as_ptr(), matches!(r, Err(RunError::StepLimit)) as i32) };
    let r = Machine::new().run(&Program::compile(b"<").unwrap(), &mut None.into_iter(), &mut Checksum::default(), 10);
    unsafe { printf(c"underflow: %d\n".as_ptr(), matches!(r, Err(RunError::TapeUnderflow { pc: 0 })) as i32) };

    for bad in [&b"[[]"[..], b"[]]", b"+[>[<-]"] {
        match Program::compile(bad) {
            Ok(_) => print_str("compiled?!\n"),
            Err(e) => {
                let (what, at) = describe(e);
                print_str(what);
                unsafe { printf(c" at %zu\n".as_ptr(), at) };
            }
        }
    }

    let busy = Program::compile(BUSY).unwrap();
    let mut sink = Checksum::default();
    let mut m = Machine::new();
    let steps = m.run(&busy, &mut None.into_iter(), &mut sink, u64::MAX).unwrap();
    unsafe {
        printf(c"busy: %llu steps, output %llu bytes, hash %08x, cells %u %u\n".as_ptr(),
            steps, sink.count, sink.hash, m.tape[4] as u32, m.tape[5] as u32)
    };
}
