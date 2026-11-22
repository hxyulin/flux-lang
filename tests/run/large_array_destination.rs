use std::cell::Cell;

enum Value<'a> {
    Short(usize, &'a Cell<usize>),
    Long(usize, usize, &'a Cell<usize>),
}

impl Value<'_> {
    fn index(&self) -> usize {
        match self {
            Self::Short(index, _) => *index,
            Self::Long(high, low, _) => high + low,
        }
    }
}

impl Drop for Value<'_> {
    fn drop(&mut self) {
        let count = match self {
            Self::Short(_, count) | Self::Long(_, _, count) => count,
        };
        count.set(count.get() + 1);
    }
}

fn next<'a>(created: &Cell<usize>, dropped: &'a Cell<usize>) -> Value<'a> {
    let index = created.get();
    created.set(index + 1);
    if index % 2 == 0 {
        Value::Short(index, dropped)
    } else {
        Value::Long(index - 1, 1, dropped)
    }
}

fn list(created: &Cell<usize>, dropped: &Cell<usize>, leave_early: bool) {
    let values = vec![
        next(created, dropped),
        {
            if leave_early {
                return;
            }
            next(created, dropped)
        },
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
        next(created, dropped), next(created, dropped), next(created, dropped),
    ];
    assert_eq!(values.len(), 32);
    assert_eq!(values.capacity(), 32);
    assert_eq!(created.get(), 32);
    assert_eq!(dropped.get(), 0);
    for (index, value) in values.iter().enumerate() {
        assert_eq!(value.index(), index);
    }
}

fn main() {
    for (leave_early, count) in [(false, 32), (true, 1)] {
        let created = Cell::new(0);
        let dropped = Cell::new(0);
        list(&created, &dropped, leave_early);
        assert_eq!(created.get(), count);
        assert_eq!(dropped.get(), count);
    }
    println!("large array elements evaluated and dropped once");
}
