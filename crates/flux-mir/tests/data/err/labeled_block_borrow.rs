fn main() {
    let r = 'escape: {
        let x = 5;
        break 'escape &x;
    };
    let _y = *r;
}
