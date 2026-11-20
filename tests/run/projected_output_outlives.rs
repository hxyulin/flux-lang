trait Files<'a> {
    type Id: 'a + Copy + PartialEq;
}

impl<'a> Files<'a> for u32 {
    type Id = u32;
}

struct View<'view, Id>(&'view Id);

impl<'view, Id: Copy + PartialEq> View<'view, Id> {
    fn render<'files>(&self, _: &'files (impl Files<'files, Id = Id> + ?Sized)) -> Id
    where
        Id: 'files,
    {
        *self.0
    }
}

fn emit<'files, 'view, F: Files<'files> + ?Sized>(files: &'files F, id: &'view F::Id) -> F::Id {
    View(id).render(files)
}

fn main() {
    let files = 0u32;
    let id = 42u32;
    println!("{}", emit(&files, &id));
}
