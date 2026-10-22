// Binders must be checked before reference-returning Fn bounds erase them.
fn shadow<'a, F: for<'a> Fn(&'a i32) -> &'a i32>(f: F) {}

fn where_shadow<'a, F>(f: F)
where
    for<'a> F: Fn(&'a i32) -> &'a i32,
{}

fn reserved<F: for<'static> Fn(&'static i32) -> &'static i32>(f: F) {}
fn anonymous<F: for<'_> Fn(&i32) -> &i32>(f: F) {}
fn bounded<F: for<'a: 'static> Fn(&'a i32) -> &'a i32>(f: F) {}

struct S<'a>(&'a i32);
impl<'a> S<'a> {
    fn shadow<F: for<'a> Fn(&'a i32) -> &'a i32>(&self, f: F) {}
}

fn output_only<F: for<'a> Fn(&i32) -> &'a i32>(f: F) {}

fn main() {}
