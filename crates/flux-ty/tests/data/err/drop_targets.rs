// `Drop` may only be implemented for structs (E0120): not for tuples or
// references. rustc also reports E0117 for the tuple, which flux does
// not check yet.

struct Element;

impl Drop for (Element, Element) {
    fn drop(&mut self) {}
}

impl Drop for &Element {
    fn drop(&mut self) {}
}

fn main() {}
