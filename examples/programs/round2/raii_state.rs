//! Observable drop order and state machines. RAII guards record their
//! drops into a shared `Log` (interior mutability through `Cell`), which
//! shows rustc's order: locals in reverse, struct fields in declaration
//! order, temporaries at the end of their statement, moved values not at
//! all, values overwritten by assignment, `mem::replace`/`take`, early
//! returns and `?`, and a `Defer` guard running a closure. Then a vending
//! machine as an enum state machine with data in its states, driven by a
//! script of events, where transitions consume the old state by value.

use core::cell::Cell;
use core::mem;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Log {
    entries: [Cell<u8>; 64],
    len: Cell<usize>,
}

impl Log {
    fn new() -> Self {
        Log { entries: [const { Cell::new(0) }; 64], len: Cell::new(0) }
    }

    fn push(&self, id: u8) {
        let n = self.len.get();
        self.entries[n].set(id);
        self.len.set(n + 1);
    }

    fn print_and_clear(&self, label: &str) {
        unsafe { printf(c"%-14.*s".as_ptr(), label.len() as i32, label.as_ptr()) };
        for e in &self.entries[..self.len.get()] {
            unsafe { printf(c" %c".as_ptr(), e.get() as i32) };
        }
        unsafe { printf(c"\n".as_ptr()) };
        self.len.set(0);
    }
}

struct Guard<'a> {
    id: u8,
    log: &'a Log,
}

impl<'a> Guard<'a> {
    fn new(id: u8, log: &'a Log) -> Self {
        Guard { id, log }
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.log.push(self.id);
    }
}

struct Pair<'a> {
    first: Guard<'a>,
    second: Guard<'a>,
}

impl Drop for Pair<'_> {
    fn drop(&mut self) {
        // Runs before the fields are dropped.
        self.first.log.push(b'P');
    }
}

struct Defer<F: FnMut()> {
    action: F,
}

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.action)();
    }
}

fn defer<F: FnMut()>(action: F) -> Defer<F> {
    Defer { action }
}

#[derive(Debug, PartialEq)]
enum StepError {
    Failed(u8),
}

fn step(log: &Log, id: u8, fail: bool) -> Result<Guard<'_>, StepError> {
    if fail { Err(StepError::Failed(id)) } else { Ok(Guard::new(id, log)) }
}

fn pipeline(log: &Log, fail_at: u8) -> Result<u8, StepError> {
    let _cleanup = defer(|| log.push(b'!'));
    let a = step(log, b'a', fail_at == 1)?;
    let b = step(log, b'b', fail_at == 2)?;
    let c = step(log, b'c', fail_at == 3)?;
    drop(b);
    Ok(a.id + c.id)
}

fn consume(g: Guard<'_>) -> u8 {
    g.id
}

// ---- A vending machine ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Water = 80,
    Juice = 150,
    Snack = 120,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Coin(u32),
    Select(Item),
    Cancel,
    Restock(u32),
}

#[derive(Debug, PartialEq, Eq)]
enum Machine {
    Idle { stock: u32 },
    Paying { stock: u32, credit: u32 },
    Vending { stock: u32, item: Item, change: u32 },
    OutOfStock,
}

#[derive(Debug, Default)]
struct Totals {
    sold: u32,
    revenue: u32,
    refunded: u32,
    rejected: u32,
}

impl Machine {
    fn next(self, event: Event, totals: &mut Totals) -> Machine {
        use Machine::*;
        match (self, event) {
            (Idle { stock } | Paying { stock, credit: 0 }, Event::Coin(c)) => Paying { stock, credit: c },
            (Paying { stock, credit }, Event::Coin(c)) => Paying { stock, credit: credit + c },
            (Paying { stock, credit }, Event::Select(item)) if credit >= item as u32 => {
                totals.sold += 1;
                totals.revenue += item as u32;
                Vending { stock: stock - 1, item, change: credit - item as u32 }
            }
            (state @ Paying { .. }, Event::Select(_)) => {
                totals.rejected += 1;
                state
            }
            (Paying { stock, credit }, Event::Cancel) => {
                totals.refunded += credit;
                Idle { stock }
            }
            (Vending { stock: 0, .. }, _) => OutOfStock,
            (Vending { stock, change, .. }, event) => {
                totals.refunded += change;
                Idle { stock }.next(event, totals)
            }
            (OutOfStock, Event::Restock(n)) | (Idle { stock: n }, Event::Restock(_)) if n > 0 => Idle { stock: n },
            (OutOfStock, Event::Coin(c)) => {
                totals.refunded += c;
                OutOfStock
            }
            (state, _) => {
                totals.rejected += 1;
                state
            }
        }
    }

    fn describe(&self) -> &'static str {
        match self {
            Machine::Idle { .. } => "idle",
            Machine::Paying { .. } => "paying",
            Machine::Vending { item: Item::Water, .. } => "vending water",
            Machine::Vending { item: Item::Juice, .. } => "vending juice",
            Machine::Vending { item: Item::Snack, .. } => "vending snack",
            Machine::OutOfStock => "out of stock",
        }
    }
}

fn main() {
    let log = Log::new();
    {
        let _a = Guard::new(b'a', &log);
        let _b = Guard::new(b'b', &log);
        let _ = Guard::new(b't', &log); // dropped at once
        let _c = Guard::new(b'c', &log);
    }
    log.print_and_clear("scope:");

    {
        let pair = Pair { first: Guard::new(b'1', &log), second: Guard::new(b'2', &log) };
        let _x = Guard::new(b'x', &log);
        let moved = pair;
        log.push(b'-');
        drop(moved);
        log.push(b'-');
    }
    log.print_and_clear("struct:");

    {
        let mut slot = Guard::new(b'o', &log);
        slot = Guard::new(b'n', &log); // the old value drops here
        log.push(b'-');
        let old = mem::replace(&mut slot, Guard::new(b'r', &log));
        log.push(consume(old));
        let mut opt = Some(Guard::new(b's', &log));
        let taken = opt.take();
        log.push(b'-');
        drop(opt);
        log.push(b'-');
        let _keep = taken;
    }
    log.print_and_clear("replace/take:");

    {
        let guards = [Guard::new(b'0', &log), Guard::new(b'1', &log), Guard::new(b'2', &log)];
        for g in guards {
            if g.id == b'1' {
                log.push(b'*');
                continue;
            }
            log.push(b'.');
        }
        log.push(b'-');
    }
    log.print_and_clear("array loop:");

    for fail_at in 0..4 {
        let r = pipeline(&log, fail_at);
        let label = match r {
            Ok(_) => "pipeline ok:",
            Err(StepError::Failed(b'a')) => "fail at a:",
            Err(StepError::Failed(b'b')) => "fail at b:",
            Err(_) => "fail at c:",
        };
        log.print_and_clear(label);
    }

    {
        let counter = Cell::new(0);
        {
            let _d1 = defer(|| counter.set(counter.get() * 10 + 1));
            let _d2 = defer(|| counter.set(counter.get() * 10 + 2));
            let mut d3 = defer(|| counter.set(counter.get() * 10 + 3));
            (d3.action)();
        }
        unsafe { printf(c"defer order:   %d\n".as_ptr(), counter.get()) };
    }

    // The vending machine.
    let script = [
        Event::Coin(100),
        Event::Select(Item::Juice),
        Event::Coin(50),
        Event::Select(Item::Juice),
        Event::Coin(200),
        Event::Select(Item::Water),
        Event::Cancel,
        Event::Coin(20),
        Event::Cancel,
        Event::Coin(100),
        Event::Coin(100),
        Event::Select(Item::Snack),
        Event::Coin(150),
        Event::Select(Item::Juice),
        Event::Coin(10),
        Event::Restock(5),
        Event::Coin(80),
        Event::Select(Item::Water),
        Event::Cancel,
    ];
    let mut totals = Totals::default();
    let mut machine = Machine::Idle { stock: 3 };
    for (i, &event) in script.iter().enumerate() {
        machine = machine.next(event, &mut totals);
        let d = machine.describe();
        unsafe { printf(c"%2zu: %.*s\n".as_ptr(), i, d.len() as i32, d.as_ptr()) };
    }
    assert!(matches!(machine, Machine::Idle { stock: 4 }));
    unsafe {
        printf(c"sold %u, revenue %u, refunded %u, rejected %u\n".as_ptr(),
            totals.sold, totals.revenue, totals.refunded, totals.rejected);
    }
}
