fn main() {
    'static: { break 'static; }
    '_: { break '_; }
    'block: { continue 'block; }
    'block: { break; }
    loop { 'block: { continue; } }
    'block: { break 'missing; }
    'block: { let f = || { break 'block; }; }
    'same: loop { 'same: { continue 'same; } }
}
