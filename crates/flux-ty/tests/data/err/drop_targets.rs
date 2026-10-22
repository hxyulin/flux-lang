// `Drop` may only be implemented for structs (E0120): not for tuples or
// references. The tuple also violates the orphan rule (E0117), since
// tuples are foreign even when their elements are local.

struct Element;

impl Drop for (Element, Element) {
    fn drop(&mut self) {}
}

impl Drop for &Element {
    fn drop(&mut self) {}
}

fn main() {}
