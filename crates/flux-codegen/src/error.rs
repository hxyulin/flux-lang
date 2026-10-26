//! Backend failures retain their stage and structured causes.
use std::{error, fmt, io};

/// The backend operation that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stage {
    Verify,
    Optimize,
    Codegen,
    Assemble,
    Sdk,
    Link,
    Write,
}

/// A backend failure. Display preserves the compiler's diagnostic text;
/// callers can inspect the stage and standard error chain separately.
#[derive(Debug)]
pub struct Error {
    stage: Stage,
    kind: Kind,
}
#[derive(Debug)]
enum Kind {
    Message(String),
    Optimize(keel_opt::OptError),
    Codegen(keel_codegen::CodegenError),
    Assemble(keel_asm::Error),
    Io { context: String, source: io::Error },
}
impl Error {
    /// Which backend operation failed.
    pub fn stage(&self) -> Stage {
        self.stage
    }
    pub(crate) fn message(stage: Stage, message: impl Into<String>) -> Self {
        Self {
            stage,
            kind: Kind::Message(message.into()),
        }
    }
    pub(crate) fn optimize(source: keel_opt::OptError) -> Self {
        Self {
            stage: Stage::Optimize,
            kind: Kind::Optimize(source),
        }
    }
    pub(crate) fn codegen(source: keel_codegen::CodegenError) -> Self {
        Self {
            stage: Stage::Codegen,
            kind: Kind::Codegen(source),
        }
    }
    pub(crate) fn assemble(source: keel_asm::Error) -> Self {
        Self {
            stage: Stage::Assemble,
            kind: Kind::Assemble(source),
        }
    }
    pub(crate) fn io(stage: Stage, context: &str, source: io::Error) -> Self {
        Self {
            stage,
            kind: Kind::Io {
                context: context.into(),
                source,
            },
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::Message(message) => f.write_str(message),
            Kind::Optimize(error) => error.fmt(f),
            Kind::Codegen(error) => error.fmt(f),
            Kind::Assemble(error) => error.fmt(f),
            Kind::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}
impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match &self.kind {
            Kind::Codegen(error) => Some(error),
            Kind::Assemble(error) => Some(error),
            Kind::Io { source, .. } => Some(source),
            // Keel optimization and linker errors do not implement Error.
            // Retain the structured optimization value and original text.
            Kind::Optimize(_) | Kind::Message(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_preserve_stage_display_and_sources() {
        use std::error::Error as _;
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Error>();
        let error = Error::io(
            Stage::Sdk,
            "cannot find SDK",
            io::Error::from(io::ErrorKind::NotFound),
        );
        assert_eq!(error.stage(), Stage::Sdk);
        assert!(error.to_string().starts_with("cannot find SDK: "));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound
        );
        let error = Error::message(Stage::Link, "undefined symbol");
        assert_eq!(error.to_string(), "undefined symbol");
        assert!(error.source().is_none());
    }
}
