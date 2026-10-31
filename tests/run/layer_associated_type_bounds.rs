trait Layer<S> {
    type Service;
    fn layer(&self, inner: S) -> Self::Service;
}

impl<T: ?Sized + Layer<S>, S> Layer<S> for &T {
    type Service = T::Service;
    fn layer(&self, service: S) -> Self::Service {
        (**self).layer(service)
    }
}

impl<S, L1, L2> Layer<S> for (L1, L2)
where
    L1: Layer<L2::Service>,
    L2: Layer<S>,
{
    type Service = L1::Service;
    fn layer(&self, service: S) -> Self::Service {
        let (l1, l2) = self;
        l1.layer(l2.layer(service))
    }
}

impl<S, L1, L2, L3> Layer<S> for (L1, L2, L3)
where
    L1: Layer<L2::Service>,
    L2: Layer<L3::Service>,
    L3: Layer<S>,
{
    type Service = L1::Service;
    fn layer(&self, service: S) -> Self::Service {
        let (l1, l2, l3) = self;
        l1.layer((l2, l3).layer(service))
    }
}

struct Add(i32);
impl Layer<i32> for Add {
    type Service = i32;
    fn layer(&self, service: i32) -> i32 {
        service + self.0
    }
}

struct Wrap;
impl Layer<i32> for Wrap {
    type Service = (i32, i32);
    fn layer(&self, service: i32) -> Self::Service {
        (service, service + 1)
    }
}

fn main() {
    println!("{}", (Add(2), Add(3)).layer(7));
    let pair = (Wrap, Add(5)).layer(11);
    println!("{} {}", pair.0, pair.1);
    println!("{}", ((Add(1), Add(2)), Add(3)).layer(4));
    println!("{}", (Add(1), Add(2), Add(3)).layer(4));
}
