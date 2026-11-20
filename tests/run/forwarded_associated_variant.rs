trait VariantAccess<'de> {
    type Error;
}

trait Seed<'de> {
    type Value;
    fn value(self, input: &'de usize) -> Self::Value;
}

trait EnumAccess<'de>: Sized {
    type Error;
    type Variant: VariantAccess<'de, Error = Self::Error>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: Seed<'de>;
}

struct Wrap<'a, X> {
    delegate: X,
    path: &'a usize,
}

impl<'de, X: VariantAccess<'de>> VariantAccess<'de> for Wrap<'_, X> {
    type Error = X::Error;
}

impl<'a, 'de, X: EnumAccess<'de> + 'a> EnumAccess<'de> for Wrap<'a, X> {
    type Error = X::Error;
    type Variant = Wrap<'a, X::Variant>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), X::Error>
    where
        V: Seed<'de>,
    {
        let path = self.path;
        self.delegate
            .variant_seed(seed)
            .map(move |(value, delegate)| (value, Wrap { delegate, path }))
    }
}

struct Input<'de>(&'de usize);

impl<'de> VariantAccess<'de> for Input<'de> {
    type Error = ();
}

impl<'de> EnumAccess<'de> for Input<'de> {
    type Error = ();
    type Variant = Self;

    fn variant_seed<V: Seed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error> {
        Ok((seed.value(self.0), self))
    }
}

struct Read;

impl<'de> Seed<'de> for Read {
    type Value = usize;

    fn value(self, input: &'de usize) -> Self::Value {
        *input
    }
}

fn main() {
    let input = 12;
    let path = 30;
    let wrapper = Wrap {
        delegate: Input(&input),
        path: &path,
    };
    let (value, variant) = wrapper.variant_seed(Read).unwrap();
    println!("{} {} {}", value, variant.delegate.0, variant.path);
}
