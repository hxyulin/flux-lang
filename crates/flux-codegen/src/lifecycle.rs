//! Rust statics can define lifecycle arrays directly. Keel reserves their
//! section names for its own function attributes. Preserve the static's data
//! and symbol through codegen, then give the assembler the typed section.

use keel_asm_ir::{Directive, Item, Program, SectionKind};
use keel_ir::{DataItem, Module, Target, section::SectionName};

use crate::{Error, Stage};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    DarwinInit,
    ElfInit,
    ElfFini,
}

impl Kind {
    fn for_section(target: Target, name: &str) -> Option<Self> {
        match (target, name) {
            (Target::Aarch64AppleDarwin, "__DATA,__mod_init_func") => Some(Self::DarwinInit),
            (Target::X86_64LinuxGnu, ".init_array") => Some(Self::ElfInit),
            (Target::X86_64LinuxGnu, ".fini_array") => Some(Self::ElfFini),
            _ => None,
        }
    }

    fn directive(self) -> Directive {
        match self {
            Self::DarwinInit => Directive::Section {
                segment: "__DATA".into(),
                name: "__mod_init_func".into(),
                kind: SectionKind::ModuleInit,
            },
            Self::ElfInit | Self::ElfFini => Directive::ElfLifecycleArray {
                fini: self == Self::ElfFini,
            },
        }
    }
}

struct Section {
    kind: Kind,
    native: SectionName,
    temporary: SectionName,
}

pub(crate) struct Sections(Vec<Section>);

impl Sections {
    pub(crate) fn prepare(module: &mut Module, target: Target) -> Result<Self, Error> {
        let mut sections: Vec<Section> = Vec::new();
        let mut next = 0;
        for data in &module.data {
            let Some(native) = data.section else { continue };
            let Some(kind) = Kind::for_section(target, native.as_str()) else {
                continue;
            };
            if data.thread_local
                || data.weak_odr
                || !data.init.as_ref().is_some_and(|init| {
                    init.align == 8
                        && !init.items.is_empty()
                        && init
                            .items
                            .iter()
                            .all(|item| matches!(item, DataItem::Ptr { addend: 0, .. }))
                })
            {
                return Err(Error::message(
                    Stage::Codegen,
                    format!(
                        "{} requires pointer-sized function addresses in {native}",
                        data.name
                    ),
                ));
            }
            if sections.iter().any(|s| s.kind == kind) {
                continue;
            }
            let temporary = loop {
                let name = match kind {
                    Kind::DarwinInit => format!("__DATA,__flux_l{next:x}"),
                    _ => format!(".flux.lifecycle.{next}"),
                };
                next += 1;
                let section = SectionName::new(&name).expect("generated section name");
                if !module.data.iter().any(|d| d.section == Some(section))
                    && !module
                        .functions
                        .iter()
                        .any(|f| f.attrs.section == Some(section))
                {
                    break section;
                }
            };
            sections.push(Section {
                kind,
                native,
                temporary,
            });
        }
        let sections = Self(sections);
        sections.rename(module, false);
        Ok(sections)
    }

    fn rename(&self, module: &mut Module, restore: bool) {
        for section in &self.0 {
            let (from, to) = if restore {
                (section.temporary, section.native)
            } else {
                (section.native, section.temporary)
            };
            for data in &mut module.data {
                if data.section == Some(from) {
                    data.section = Some(to);
                }
            }
        }
    }

    pub(crate) fn restore(&self, module: &mut Module) {
        self.rename(module, true);
    }

    pub(crate) fn rewrite(&self, program: &mut Program) {
        if self.0.is_empty() {
            return;
        }
        for item in &mut program.items {
            let Item::Directive(directive) = item else {
                continue;
            };
            for section in &self.0 {
                let matches = match directive {
                    Directive::Section { segment, name, .. } => section
                        .temporary
                        .as_str()
                        .split_once(',')
                        .is_some_and(|(s, n)| s == segment && n == name),
                    Directive::ElfSection { name, .. } => section.temporary.as_str() == name,
                    _ => false,
                };
                if matches {
                    *directive = section.kind.directive();
                    break;
                }
            }
        }
    }
}
