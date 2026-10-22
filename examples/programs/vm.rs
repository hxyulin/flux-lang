//! A small stack-based bytecode VM: programs are `const` arrays of
//! instructions with jumps, calls and returns, run by a match loop on a
//! fixed-size operand stack and call stack, with errors as `Result`.
//! Runs factorial, an iterative fibonacci, a recursive gcd and some
//! programs that fail on purpose.
//!
//! Rewritten for flux from `original/vm.rs`: no `Debug` derive, no const
//! generics (the stack's capacity is a constant), no `impl Fn` argument
//! (a type parameter instead), no `checked_div`/`checked_rem`, no `&CStr`,
//! no iterating an array by value.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Push(i64),
    Pop,
    Dup,
    Swap,
    Over,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Eq,
    Not,
    Jmp(usize),
    Jz(usize),
    Call(usize),
    Ret,
    Load(u8),
    Store(u8),
    Print,
    Halt,
}

#[derive(Clone, Copy, PartialEq)]
enum VmError {
    StackOverflow,
    StackUnderflow,
    DivideByZero,
    BadJump(usize),
    StepLimit,
}

const STACK: usize = 64;
const FRAMES: usize = 32;
const STEP_LIMIT: u32 = 100_000;

struct Stack {
    items: [i64; STACK],
    len: usize,
}

impl Stack {
    fn new() -> Self {
        Stack { items: [0; STACK], len: 0 }
    }

    fn push(&mut self, v: i64) -> Result<(), VmError> {
        if self.len == STACK {
            return Err(VmError::StackOverflow);
        }
        self.items[self.len] = v;
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<i64, VmError> {
        if self.len == 0 {
            return Err(VmError::StackUnderflow);
        }
        self.len -= 1;
        Ok(self.items[self.len])
    }

    fn peek(&self, depth: usize) -> Result<i64, VmError> {
        if depth >= self.len {
            return Err(VmError::StackUnderflow);
        }
        Ok(self.items[self.len - 1 - depth])
    }
}

struct Vm<'p> {
    code: &'p [Op],
    pc: usize,
    stack: Stack,
    frames: [usize; FRAMES],
    depth: usize,
    vars: [i64; 8],
    steps: u32,
}

impl<'p> Vm<'p> {
    fn new(code: &'p [Op]) -> Self {
        Vm { code, pc: 0, stack: Stack::new(), frames: [0; FRAMES], depth: 0, vars: [0; 8], steps: 0 }
    }

    fn binary<F: Fn(i64, i64) -> Result<i64, VmError>>(&mut self, f: F) -> Result<(), VmError> {
        let b = self.stack.pop()?;
        let a = self.stack.pop()?;
        self.stack.push(f(a, b)?)
    }

    fn run(&mut self) -> Result<Option<i64>, VmError> {
        loop {
            self.steps += 1;
            if self.steps > STEP_LIMIT {
                return Err(VmError::StepLimit);
            }
            let op = *self.code.get(self.pc).ok_or(VmError::BadJump(self.pc))?;
            self.pc += 1;
            match op {
                Op::Push(v) => self.stack.push(v)?,
                Op::Pop => {
                    self.stack.pop()?;
                }
                Op::Dup => self.stack.push(self.stack.peek(0)?)?,
                Op::Over => self.stack.push(self.stack.peek(1)?)?,
                Op::Swap => {
                    let b = self.stack.pop()?;
                    let a = self.stack.pop()?;
                    self.stack.push(b)?;
                    self.stack.push(a)?;
                }
                Op::Add => self.binary(|a, b| Ok(a.wrapping_add(b)))?,
                Op::Sub => self.binary(|a, b| Ok(a.wrapping_sub(b)))?,
                Op::Mul => self.binary(|a, b| Ok(a.wrapping_mul(b)))?,
                Op::Div => self.binary(|a, b| if b == 0 { Err(VmError::DivideByZero) } else { Ok(a / b) })?,
                Op::Mod => self.binary(|a, b| if b == 0 { Err(VmError::DivideByZero) } else { Ok(a % b) })?,
                Op::Lt => self.binary(|a, b| Ok((a < b) as i64))?,
                Op::Eq => self.binary(|a, b| Ok((a == b) as i64))?,
                Op::Not => {
                    let a = self.stack.pop()?;
                    self.stack.push((a == 0) as i64)?;
                }
                Op::Jmp(t) => self.pc = t,
                Op::Jz(t) => {
                    if self.stack.pop()? == 0 {
                        self.pc = t;
                    }
                }
                Op::Call(t) => {
                    if self.depth == FRAMES {
                        return Err(VmError::StackOverflow);
                    }
                    self.frames[self.depth] = self.pc;
                    self.depth += 1;
                    self.pc = t;
                }
                Op::Ret => {
                    if self.depth == 0 {
                        return Ok(self.stack.peek(0).ok());
                    }
                    self.depth -= 1;
                    self.pc = self.frames[self.depth];
                }
                Op::Load(i) => self.stack.push(self.vars[i as usize])?,
                Op::Store(i) => self.vars[i as usize] = self.stack.pop()?,
                Op::Print => {
                    let v = self.stack.peek(0)?;
                    unsafe {
                        printf(c"  print %lld\n".as_ptr(), v);
                    }
                }
                Op::Halt => return Ok(self.stack.peek(0).ok()),
            }
        }
    }
}

use Op::*;

/// 10! by a counting loop: acc in var 0, n in var 1.
const FACTORIAL: [Op; 17] = [
    Push(1), Store(0), Push(10), Store(1),
    // 4:
    Load(1), Jz(15),
    Load(0), Load(1), Mul, Store(0),
    Load(1), Push(1), Sub, Store(1), Jmp(4),
    // 15:
    Load(0), Halt,
];

/// The first fibonacci numbers, printing each, by keeping two on the stack.
const FIBONACCI: [Op; 16] = [
    Push(0), Push(1), Push(12), Store(0),
    // 4: loop while counter != 0
    Load(0), Jz(15),
    Load(0), Push(1), Sub, Store(0),
    Print, Swap, Over, Add, Jmp(4),
    // 15:
    Halt,
];

/// gcd(1071, 462) with a recursive subroutine at 4.
const GCD: [Op; 12] = [
    Push(1071), Push(462), Call(4), Halt,
    // 4: gcd(a, b) = if b == 0 { a } else { gcd(b, a % b) }
    Dup, Jz(10), Swap, Over, Mod, Jmp(4),
    // 10:
    Pop, Ret,
];

/// (7 < 9) + !(3 == 3) + (5 == 5)
const COMPARE: [Op; 13] = [Push(7), Push(9), Lt, Push(3), Push(3), Eq, Not, Add, Push(5), Dup, Eq, Add, Ret];

const DIVIDE_BY_ZERO: [Op; 4] = [Push(1), Push(0), Div, Halt];
const UNDERFLOW: [Op; 3] = [Push(1), Add, Halt];
const RUNAWAY: [Op; 2] = [Call(0), Halt];
const FOREVER: [Op; 1] = [Jmp(0)];

fn describe(e: VmError) -> *const i8 {
    match e {
        VmError::StackOverflow => c"stack overflow".as_ptr(),
        VmError::StackUnderflow => c"stack underflow".as_ptr(),
        VmError::DivideByZero => c"division by zero".as_ptr(),
        VmError::BadJump(_) => c"bad jump".as_ptr(),
        VmError::StepLimit => c"step limit reached".as_ptr(),
    }
}

fn main() {
    let programs: [(*const i8, &[Op]); 8] = [
        (c"factorial".as_ptr(), &FACTORIAL),
        (c"fibonacci".as_ptr(), &FIBONACCI),
        (c"gcd".as_ptr(), &GCD),
        (c"compare".as_ptr(), &COMPARE),
        (c"divide by zero".as_ptr(), &DIVIDE_BY_ZERO),
        (c"underflow".as_ptr(), &UNDERFLOW),
        (c"runaway recursion".as_ptr(), &RUNAWAY),
        (c"infinite loop".as_ptr(), &FOREVER),
    ];
    let mut ok = 0;
    for &(name, code) in &programs {
        unsafe {
            printf(c"%s:\n".as_ptr(), name);
        }
        let mut vm = Vm::new(code);
        match vm.run() {
            Ok(Some(v)) => {
                ok += 1;
                unsafe { printf(c"  result %lld after %u steps\n".as_ptr(), v, vm.steps) };
            }
            Ok(None) => unsafe {
                printf(c"  empty stack\n".as_ptr());
            },
            Err(e) => unsafe {
                printf(c"  error: %s at pc %zu\n".as_ptr(), describe(e), vm.pc);
            },
        }
    }
    unsafe { printf(c"%d programs succeeded\n".as_ptr(), ok) };
}
