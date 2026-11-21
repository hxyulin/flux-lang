struct Guard<F>
where
    F: Fn(&mut u32) -> &mut u32,
{
    project: F,
}

impl<F> Guard<F>
where
    F: Fn(&mut u32) -> &'static mut u32,
{
    fn take(self) -> F {
        self.project
    }
}

fn main() {}
