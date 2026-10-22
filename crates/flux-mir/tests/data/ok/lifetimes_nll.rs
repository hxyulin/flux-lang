struct Node<'a> {
    value: i32,
    next: Option<&'a Node<'a>>,
}

fn sum_list(mut node: &Node<'_>) -> i32 {
    let mut total = node.value;
    while let Some(next) = node.next {
        total += next.value;
        node = next;
    }
    total
}

struct Counter {
    counts: [i32; 4],
    total: i32,
}

impl Counter {
    fn slot(&mut self, i: usize) -> &mut i32 {
        &mut self.counts[i]
    }

    fn bump(&mut self, i: usize) {
        *self.slot(i) += 1;
        self.total += 1;
    }

    fn max_ref(&self) -> &i32 {
        let mut best = &self.counts[0];
        let mut i = 1;
        while i < 4 {
            if self.counts[i] > *best {
                best = &self.counts[i];
            }
            i += 1;
        }
        best
    }
}

struct Split<'a> {
    left: &'a mut i32,
    right: &'a mut i32,
}

fn split(p: &mut (i32, i32)) -> Split<'_> {
    Split {
        left: &mut p.0,
        right: &mut p.1,
    }
}

fn find(v: &[i32], target: i32) -> Option<&i32> {
    let mut i = 0;
    while i < v.len() {
        if v[i] == target {
            return Some(&v[i]);
        }
        i += 1;
    }
    None
}

fn first_or<'a>(v: &'a [i32], default: &'a i32) -> &'a i32 {
    if v.is_empty() { default } else { &v[0] }
}

fn swap_refs<'a>(a: &mut &'a i32, b: &mut &'a i32) {
    let t = *a;
    *a = *b;
    *b = t;
}

fn apply<F: Fn(&i32) -> i32>(f: F, x: &i32) -> i32 {
    f(x)
}

fn main() {
    let tail = Node { value: 3, next: None };
    let mid = Node { value: 2, next: Some(&tail) };
    let head = Node { value: 1, next: Some(&mid) };
    let mut total = sum_list(&head);
    let mut c = Counter { counts: [0; 4], total: 0 };
    c.bump(1);
    c.bump(1);
    c.bump(2);
    total += *c.max_ref();
    c.bump(3);
    let mut p = (1, 2);
    {
        let s = split(&mut p);
        *s.left += 10;
        *s.right += 20;
    }
    total += p.0 + p.1;
    let v = [4, 5, 6];
    if let Some(r) = find(&v, 5) {
        total += *r;
    }
    let d = 0;
    total += *first_or(&v, &d);
    let x = 1;
    let y = 2;
    let mut a = &x;
    let mut b = &y;
    swap_refs(&mut a, &mut b);
    total += *a * 10 + *b;
    let k = 3;
    total += apply(|r| *r + k, &x);
    let mut acc = 0;
    let mut add = |r: &i32| acc += *r;
    add(&x);
    add(&y);
    total += acc;
    if total == 0 {
        loop {}
    }
}
