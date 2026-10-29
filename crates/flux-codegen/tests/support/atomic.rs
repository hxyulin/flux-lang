//! Inspect the probe's functions independently of reachable runtime atomics.
use keel_ir::{Function, Module};

pub fn named_functions<'a>(module: &'a Module, name: &str) -> Vec<&'a Function> {
    let segment = format!("{}{name}", name.len());
    let functions: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.name == name || function.name.contains(&segment))
        .collect();
    assert!(!functions.is_empty(), "no functions selected for {name}");
    assert!(
        functions
            .iter()
            .all(|function| function.layout_insts().next().is_some()),
        "empty probe function for {name}"
    );
    functions
}

pub fn function_assembly(module: &Module, selected: &[&Function], assembly: &str) -> String {
    let labels: std::collections::BTreeMap<_, _> = module
        .functions
        .iter()
        .flat_map(|function| {
            [
                (format!("{}:", function.name), function),
                (format!("_{}:", function.name), function),
            ]
        })
        .collect();
    let mut active = false;
    let mut found = std::collections::BTreeSet::new();
    let mut output = String::new();
    for line in assembly.lines() {
        if let Some(function) = labels.get(line.trim()) {
            active = selected
                .iter()
                .any(|selected| selected.name == function.name);
            if active {
                found.insert(function.name.as_str());
            }
        }
        if active {
            output.push_str(line);
            output.push('\n');
        }
    }
    for function in selected {
        assert!(
            found.contains(function.name.as_str()),
            "missing assembly for {}",
            function.name
        );
    }
    assert!(!output.is_empty(), "no selected function assembly");
    output
}

#[allow(dead_code, reason = "only bridge-spoof targets check ordinary calls")]
pub fn assert_called(module: &Module, target: &Function) {
    assert!(
        module.functions.iter().any(|function| {
            function.layout_insts().any(|(_, inst)| matches!(
            function.inst_data(inst),
            keel_ir::InstData::Call { func, .. } if function.callee(*func).name == target.name
        ))
        }),
        "no ordinary call to {}",
        target.name
    );
}
