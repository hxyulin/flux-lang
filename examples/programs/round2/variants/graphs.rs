//! flux variant of ../graphs.rs, minimally rewritten to compile with flux:
//! Defines its own Reverse (derived Ord) instead of core::cmp::Reverse;
//! type Matrix = [[u32; 8]; 8] instead of [[u32; V]; V] (works around
//! bugs/alias_array_const_len.rs).
//!
//! Graph algorithms over fixed-size adjacency matrices: a generic binary
//! heap `BinaryHeap<T: Ord, N>` (a max-heap; `Reverse` makes it a
//! min-heap), Dijkstra's shortest paths with path reconstruction, Kahn's
//! topological sort with a ring-buffer queue, cycle detection, and
//! connected components by depth-first search with an explicit stack.

use core::cmp::Ordering;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Reverse<T>(T);

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

/// A binary max-heap stored in an array.
struct BinaryHeap<T, const N: usize> {
    data: [T; N],
    len: usize,
}

impl<T: Ord + Copy + Default, const N: usize> BinaryHeap<T, N> {
    fn new() -> Self {
        BinaryHeap { data: [T::default(); N], len: 0 }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn peek(&self) -> Option<&T> {
        if self.len == 0 { None } else { Some(&self.data[0]) }
    }

    fn push(&mut self, item: T) -> bool {
        if self.len == N {
            return false;
        }
        let mut i = self.len;
        self.data[i] = item;
        self.len += 1;
        while i > 0 {
            let parent = (i - 1) / 2;
            if self.data[i] <= self.data[parent] {
                break;
            }
            self.data.swap(i, parent);
            i = parent;
        }
        true
    }

    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        self.data.swap(0, self.len);
        let top = self.data[self.len];
        let mut i = 0;
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            let mut largest = i;
            if l < self.len && self.data[l] > self.data[largest] {
                largest = l;
            }
            if r < self.len && self.data[r] > self.data[largest] {
                largest = r;
            }
            if largest == i {
                break;
            }
            self.data.swap(i, largest);
            i = largest;
        }
        Some(top)
    }
}

/// Drains a heap in priority order.
impl<T: Ord + Copy + Default, const N: usize> Iterator for BinaryHeap<T, N> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.pop()
    }
}

const V: usize = 8;
const INF: u32 = u32::MAX;

type Matrix = [[u32; 8]; 8];

/// A node and its tentative distance, ordered by distance then node.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct State {
    cost: u32,
    node: usize,
}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cost.cmp(&other.cost).then_with(|| self.node.cmp(&other.node))
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct ShortestPaths {
    dist: [u32; V],
    prev: [Option<usize>; V],
}

impl ShortestPaths {
    /// Writes the path to `target` into `out`, returning its length.
    fn path_to(&self, target: usize, out: &mut [usize; V]) -> usize {
        if self.dist[target] == INF {
            return 0;
        }
        let mut len = 0;
        let mut at = Some(target);
        while let Some(node) = at {
            out[len] = node;
            len += 1;
            at = self.prev[node];
        }
        out[..len].reverse();
        len
    }
}

fn dijkstra(graph: &Matrix, source: usize) -> ShortestPaths {
    let mut dist = [INF; V];
    let mut prev = [None; V];
    let mut heap: BinaryHeap<Reverse<State>, 64> = BinaryHeap::new();
    dist[source] = 0;
    heap.push(Reverse(State { cost: 0, node: source }));
    let mut pops = 0;
    while let Some(Reverse(State { cost, node })) = heap.pop() {
        pops += 1;
        if cost > dist[node] {
            continue;
        }
        for (next, &weight) in graph[node].iter().enumerate() {
            if weight == 0 {
                continue;
            }
            let candidate = cost + weight;
            if candidate < dist[next] {
                dist[next] = candidate;
                prev[next] = Some(node);
                heap.push(Reverse(State { cost: candidate, node: next }));
            }
        }
    }
    unsafe { printf(c"dijkstra from %zu: %d heap pops\n".as_ptr(), source, pops) };
    ShortestPaths { dist, prev }
}

/// A fixed-capacity FIFO queue.
struct Queue<const N: usize> {
    buf: [usize; N],
    head: usize,
    len: usize,
}

impl<const N: usize> Queue<N> {
    fn new() -> Self {
        Queue { buf: [0; N], head: 0, len: 0 }
    }
    fn push_back(&mut self, x: usize) {
        assert!(self.len < N);
        self.buf[(self.head + self.len) % N] = x;
        self.len += 1;
    }
    fn pop_front(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let x = self.buf[self.head];
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(x)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TopoError {
    Cycle { remaining: usize },
}

/// Kahn's algorithm; edges are `deps[a][b] != 0` meaning a -> b.
fn topo_sort(deps: &Matrix, order: &mut [usize; V]) -> Result<usize, TopoError> {
    let mut indegree = [0usize; V];
    for row in deps.iter() {
        for (b, &e) in row.iter().enumerate() {
            if e != 0 {
                indegree[b] += 1;
            }
        }
    }
    let mut queue: Queue<V> = Queue::new();
    for (v, _) in indegree.iter().enumerate().filter(|&(_, &d)| d == 0) {
        queue.push_back(v);
    }
    let mut n = 0;
    while let Some(v) = queue.pop_front() {
        order[n] = v;
        n += 1;
        for b in 0..V {
            if deps[v][b] != 0 {
                indegree[b] -= 1;
                if indegree[b] == 0 {
                    queue.push_back(b);
                }
            }
        }
    }
    if n == V { Ok(n) } else { Err(TopoError::Cycle { remaining: V - n }) }
}

fn components(graph: &Matrix) -> [usize; V] {
    let mut label = [usize::MAX; V];
    let mut next_label = 0;
    let mut stack = [0usize; V * V];
    for start in 0..V {
        if label[start] != usize::MAX {
            continue;
        }
        let mut sp = 0;
        stack[sp] = start;
        sp += 1;
        while sp > 0 {
            sp -= 1;
            let v = stack[sp];
            if label[v] != usize::MAX {
                continue;
            }
            label[v] = next_label;
            for w in (0..V).filter(|&w| graph[v][w] != 0 || graph[w][v] != 0) {
                if label[w] == usize::MAX {
                    stack[sp] = w;
                    sp += 1;
                }
            }
        }
        next_label += 1;
    }
    label
}

fn add_edge(m: &mut Matrix, a: usize, b: usize, w: u32) {
    m[a][b] = w;
    m[b][a] = w;
}

fn main() {
    // Heap sanity: a max-heap, and a min-heap through Reverse.
    let mut h: BinaryHeap<i32, 16> = BinaryHeap::new();
    for x in [5, -3, 9, 0, 9, 12, 7, 1] {
        h.push(x);
    }
    unsafe { printf(c"peek %d len %zu:".as_ptr(), *h.peek().unwrap(), h.len()) };
    for x in h {
        unsafe { printf(c" %d".as_ptr(), x) };
    }
    let mut mins: BinaryHeap<Reverse<u8>, 8> = BinaryHeap::new();
    for b in *b"heapsort" {
        mins.push(Reverse(b));
    }
    unsafe { printf(c"\nmin-heap: ".as_ptr()) };
    while let Some(Reverse(b)) = mins.pop() {
        unsafe { printf(c"%c".as_ptr(), b as i32) };
    }
    unsafe { printf(c" (empty %d)\n".as_ptr(), mins.is_empty() as i32) };

    // Dijkstra on a weighted undirected graph.
    let mut g: Matrix = [[0; V]; V];
    add_edge(&mut g, 0, 1, 4);
    add_edge(&mut g, 0, 2, 1);
    add_edge(&mut g, 2, 1, 2);
    add_edge(&mut g, 1, 3, 5);
    add_edge(&mut g, 2, 3, 8);
    add_edge(&mut g, 3, 4, 3);
    add_edge(&mut g, 4, 5, 1);
    add_edge(&mut g, 3, 5, 6);
    // 6 and 7 are a separate component.
    add_edge(&mut g, 6, 7, 2);

    let sp = dijkstra(&g, 0);
    let mut path = [0usize; V];
    for target in 0..V {
        let n = sp.path_to(target, &mut path);
        if n == 0 {
            unsafe { printf(c"  0 -> %zu: unreachable\n".as_ptr(), target) };
            continue;
        }
        unsafe { printf(c"  0 -> %zu: cost %2u via".as_ptr(), target, sp.dist[target]) };
        for &p in &path[..n] {
            unsafe { printf(c" %zu".as_ptr(), p) };
        }
        unsafe { printf(c"\n".as_ptr()) };
    }
    let reachable = sp.dist.iter().filter(|&&d| d != INF).count();
    let farthest = sp.dist.iter().copied().filter(|&d| d != INF).max().unwrap_or(0);

    let labels = components(&g);
    let ncomp = labels.iter().max().map_or(0, |&m| m + 1);
    unsafe { printf(c"reachable %zu, farthest %u, components %zu\n".as_ptr(), reachable, farthest, ncomp) };

    // Topological sort of a build graph, then with a cycle added.
    let names = ["core", "alloc", "std", "syntax", "hir", "ty", "mir", "cli"];
    let mut deps: Matrix = [[0; V]; V];
    for (a, b) in [(0, 1), (1, 2), (0, 3), (3, 4), (4, 5), (5, 6), (6, 7), (2, 7), (0, 5)] {
        deps[a][b] = 1;
    }
    let mut order = [0usize; V];
    match topo_sort(&deps, &mut order) {
        Ok(n) => {
            unsafe { printf(c"build order:".as_ptr()) };
            for &i in &order[..n] {
                let name = names[i];
                unsafe { printf(c" %.*s".as_ptr(), name.len() as i32, name.as_ptr()) };
            }
            unsafe { printf(c"\n".as_ptr()) };
        }
        Err(e) => unsafe { printf(c"unexpected %d\n".as_ptr(), matches!(e, TopoError::Cycle { .. }) as i32); },
    }
    deps[7][3] = 1;
    let result = topo_sort(&deps, &mut order);
    let remaining = match result {
        Err(TopoError::Cycle { remaining }) => remaining,
        Ok(_) => 0,
    };
    unsafe { printf(c"with a cycle: %zu nodes left\n".as_ptr(), remaining) };

    let code = sp.dist.iter().filter(|&&d| d != INF).fold(0u32, |a, &d| a * 3 + d) as i32;
    unsafe { exit(code % 256) }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}
