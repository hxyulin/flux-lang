//! An inventory report over a table of records built by a `macro_rules!`
//! macro: sorting with `sort_by`, `sort_by_key` and `Ordering::then_with`,
//! `binary_search_by_key` lookups, grouping consecutive runs with
//! `chunk_by`-style windows, running totals with `scan`, moving averages
//! with `windows`, pagination with `chunks`, `impl Iterator` returns that
//! borrow the table, closures capturing by reference, by mutable reference
//! and by move, `Option`/`Result` combinators, and a parser for restock
//! orders (`"sku:qty"` lines) with a custom error type.

use core::cmp::{Ordering, Reverse};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Category {
    Tools,
    Garden,
    Kitchen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Item {
    sku: u32,
    name: &'static str,
    category: Category,
    price_cents: u32,
    qty: u32,
}

impl Item {
    fn value(&self) -> u64 {
        self.price_cents as u64 * self.qty as u64
    }
}

macro_rules! inventory {
    ($($sku:literal $name:literal $cat:ident $price:literal x $qty:literal;)*) => {
        [$(Item { sku: $sku, name: $name, category: Category::$cat, price_cents: $price, qty: $qty }),*]
    };
}

macro_rules! print {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {
        unsafe { printf(concat!($fmt, "\0").as_ptr() as *const i8 $(, $arg)*); }
    };
}

fn print_str(s: &str) {
    print!("%.*s", s.len() as i32, s.as_ptr());
}

fn category_name(c: Category) -> &'static str {
    match c {
        Category::Tools => "tools",
        Category::Garden => "garden",
        Category::Kitchen => "kitchen",
    }
}

fn low_stock(items: &[Item], threshold: u32) -> impl Iterator<Item = &Item> + '_ {
    items.iter().filter(move |i| i.qty < threshold)
}

fn names_in<'a>(items: &'a [Item], cat: Category) -> impl Iterator<Item = &'static str> + 'a {
    items.iter().filter(move |i| i.category == cat).map(|i| i.name)
}

#[derive(Debug, PartialEq, Eq)]
enum OrderError<'a> {
    Malformed(&'a str),
    BadQuantity(&'a str),
    UnknownSku(u32),
}

fn parse_u32(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 9 {
        return None;
    }
    s.bytes().try_fold(0u32, |n, b| if b.is_ascii_digit() { Some(n * 10 + (b - b'0') as u32) } else { None })
}

/// Applies `"sku:qty"` lines to the (sku-sorted) inventory.
fn restock<'a>(items: &mut [Item], orders: &'a str) -> Result<u32, OrderError<'a>> {
    let mut added = 0;
    for line in orders.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (sku, qty) = line.split_once(':').ok_or(OrderError::Malformed(line))?;
        let sku = parse_u32(sku.trim()).ok_or(OrderError::Malformed(line))?;
        let qty = parse_u32(qty.trim()).filter(|&q| q > 0).ok_or(OrderError::BadQuantity(line))?;
        let idx = items.binary_search_by_key(&sku, |i| i.sku).map_err(|_| OrderError::UnknownSku(sku))?;
        items[idx].qty += qty;
        added += qty;
    }
    Ok(added)
}

fn main() {
    let mut items = inventory! {
        1040 "hammer"        Tools   1999 x 12;
        1003 "screwdriver"   Tools    799 x 40;
        2001 "rake"          Garden  2450 x 3;
        3100 "whisk"         Kitchen  550 x 25;
        2010 "hose"          Garden  3999 x 0;
        3007 "pan"           Kitchen 4200 x 7;
        1100 "wrench"        Tools   1299 x 9;
        3050 "ladle"         Kitchen  675 x 2;
        2050 "trowel"        Garden   899 x 18;
        1200 "saw"           Tools   2799 x 4;
    };

    // By category, then by descending value, then by name.
    items.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| b.value().cmp(&a.value()))
            .then_with(|| a.name.cmp(b.name))
    });
    print!("%-6s %-12s %8s %4s %10s\n", c"sku".as_ptr(), c"name".as_ptr(), c"price".as_ptr(), c"qty".as_ptr(), c"value".as_ptr());
    let mut current = None;
    for item in &items {
        if current != Some(item.category) {
            current = Some(item.category);
            print!("-- ");
            print_str(category_name(item.category));
            print!("\n");
        }
        print!("%-6u %-12.*s %5u.%02u %4u %7llu.%02llu\n", item.sku, item.name.len() as i32, item.name.as_ptr(),
            item.price_cents / 100, item.price_cents % 100, item.qty, item.value() / 100, item.value() % 100);
    }

    // Totals per category, from consecutive runs.
    let mut start = 0;
    while start < items.len() {
        let cat = items[start].category;
        let run = items[start..].iter().take_while(|i| i.category == cat).count();
        let group = &items[start..start + run];
        let total: u64 = group.iter().map(Item::value).sum();
        let units: u32 = group.iter().map(|i| i.qty).sum();
        print_str(category_name(cat));
        print!(": %zu items, %u units, %llu.%02llu\n", run, units, total / 100, total % 100);
        start += run;
    }

    // Low stock, through a borrowing iterator; names per category.
    print!("low stock:");
    for item in low_stock(&items, 5) {
        print!(" ");
        print_str(item.name);
    }
    print!("\nkitchen:");
    names_in(&items, Category::Kitchen).for_each(|n| {
        print!(" ");
        print_str(n);
    });
    print!("\n");

    // Sorted by sku for lookups; restock orders.
    items.sort_by_key(|i| i.sku);
    let orders = "
        # weekly restock
        2010: 25
        3050:10
        1200 : 6
    ";
    match restock(&mut items, orders) {
        Ok(n) => print!("restocked %u units\n", n),
        Err(e) => print!("restock failed: %d\n", matches!(e, OrderError::UnknownSku(_)) as i32),
    }
    let bad_orders = ["1040 5", "1040:0", "9999:1", "1040:x1", "1040:7\n3007:"];
    for o in bad_orders {
        let before = items;
        let r = restock(&mut items, o);
        let kind = match &r {
            Ok(_) => "ok",
            Err(OrderError::Malformed(_)) => "malformed",
            Err(OrderError::BadQuantity(_)) => "bad quantity",
            Err(OrderError::UnknownSku(_)) => "unknown sku",
        };
        let changed = items.iter().zip(before.iter()).filter(|(a, b)| a != b).count();
        print_str(kind);
        print!(" (%zu changed)\n", changed);
    }

    let find = |sku: u32| items.binary_search_by_key(&sku, |i| i.sku).ok().map(|i| items[i]);
    let hose = find(2010).map_or(0, |i| i.qty);
    let missing = find(4242).map(|i| i.name).unwrap_or("none");
    print!("hose qty %u, sku 4242: ", hose);
    print_str(missing);
    print!("\n");

    // Running total of value in sku order, and pages of three.
    let running: [u64; 10] = {
        let mut out = [0u64; 10];
        for (slot, v) in out.iter_mut().zip(items.iter().scan(0u64, |acc, i| {
            *acc += i.value();
            Some(*acc)
        })) {
            *slot = v;
        }
        out
    };
    print!("running value:");
    for v in running {
        print!(" %llu", v / 100);
    }
    print!("\n");
    let avg3: [u32; 8] = core::array::from_fn(|k| items[k..k + 3].iter().map(|i| i.price_cents).sum::<u32>() / 3);
    let max_jump = items.windows(2).map(|w| w[1].price_cents.abs_diff(w[0].price_cents)).max().unwrap_or(0);
    print!("3-avg prices:");
    avg3.iter().for_each(|a| print!(" %u", *a));
    print!(", biggest neighbour jump %u\n", max_jump);
    for (page, chunk) in items.chunks(3).enumerate() {
        print!("page %zu:", page + 1);
        for i in chunk {
            print!(" %u", i.sku);
        }
        print!("\n");
    }

    // Most valuable three, through a Reverse key.
    let mut by_value = items;
    by_value.sort_by_key(|i| (Reverse(i.value()), i.sku));
    let mut seen = 0;
    let mut record = |name: &str| {
        seen += 1;
        print!("#%d ", seen);
        print_str(name);
        print!("\n");
    };
    by_value.iter().take(3).for_each(|i| record(i.name));
    let total: u64 = items.iter().map(Item::value).sum();
    let expensive = items.iter().filter(|i| i.price_cents > 2000).count();
    let all_named = items.iter().all(|i| !i.name.is_empty());
    let ordering = items[0].sku.cmp(&items[1].sku);
    assert!(all_named && ordering == Ordering::Less);
    unsafe { exit((total % 251) as i32 + expensive as i32) }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}
