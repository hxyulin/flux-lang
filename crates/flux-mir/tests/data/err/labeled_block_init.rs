fn main() {
    let x;
    'init: {
        if true { break 'init; }
        x = 3;
    }
    let _y = x;
}
