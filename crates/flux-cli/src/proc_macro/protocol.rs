//! Wire schema for the toolchain proc-macro server's bidirectional protocol.
//! Field/variant order is wire-significant. Based on rust-analyzer at rustc
//! 58cf8f9c35b355199727627d07ae34a4e2846d67 (MIT OR Apache-2.0):
//! <https://github.com/rust-lang/rust/tree/58cf8f9c35b355199727627d07ae34a4e2846d67/src/tools/rust-analyzer/crates/proc-macro-api>
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap as HashMap, BTreeSet as HashSet};
use std::ops::Range;
use std::path::PathBuf as Utf8PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProcMacroKind {
    CustomDerive,
    Attr,
    Bang,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum SpanMode {
    Id,
    RustAnalyzer,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub span_mode: SpanMode,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PanicMessage(pub String);
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct FlatTree {
    pub subtree: Vec<u32>,
    pub literal: Vec<u32>,
    pub punct: Vec<u32>,
    pub ident: Vec<u32>,
    pub token_tree: Vec<u32>,
    pub text: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubRequest {
    FilePath {
        file_id: u32,
    },
    SourceText {
        file_id: u32,
        ast_id: u32,
        start: u32,
        end: u32,
    },
    LocalFilePath {
        file_id: u32,
    },
    LineColumn {
        file_id: u32,
        ast_id: u32,
        offset: u32,
    },
    ByteRange {
        file_id: u32,
        ast_id: u32,
        start: u32,
        end: u32,
    },
    SpanSource {
        file_id: u32,
        ast_id: u32,
        start: u32,
        end: u32,
        ctx: u32,
    },
    SpanParent {
        file_id: u32,
        ast_id: u32,
        start: u32,
        end: u32,
        ctx: u32,
    },
    SpanJoin {
        file_id: u32,
        ast_id_first: u32,
        start_first: u32,
        end_first: u32,
        ctx_first: u32,
        ast_id_second: u32,
        start_second: u32,
        end_second: u32,
        ctx_second: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubResponse {
    FilePathResult {
        name: String,
    },
    SourceTextResult {
        text: Option<String>,
    },
    LocalFilePathResult {
        name: Option<String>,
    },
    /// Line and column are 1-based.
    LineColumnResult {
        line: u32,
        column: u32,
    },
    ByteRangeResult {
        range: Range<usize>,
    },
    SpanSourceResult {
        file_id: u32,
        ast_id: u32,
        start: u32,
        end: u32,
        ctx: u32,
    },
    SpanParentResult {
        parent_span: Option<ParentSpan>,
    },
    SpanJoinResult {
        span: Option<SpanJoin>,
    },
    Cancel {
        reason: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParentSpan {
    pub file_id: u32,
    pub ast_id: u32,
    pub start: u32,
    pub end: u32,
    pub ctx: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpanJoin {
    pub ast_id: u32,
    pub start: u32,
    pub end: u32,
    pub ctx: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Response(Response),
    SubRequest(SubRequest),
    SubResponse(SubResponse),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    ListMacros(ListMacros),
    ExpandMacro(Box<ExpandMacro>),
    ApiVersionCheck(ApiVersionCheck),
    SetConfig(ServerConfig),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    ListMacros(Result<Vec<(String, ProcMacroKind)>, String>),
    ApiVersionCheck(u32),
    SetConfig(ServerConfig),
    ExpandMacro(Result<ExpandMacroResponse, PanicMessage>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListMacros {
    pub dylib_path: Utf8PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiVersionCheck {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExpandMacro {
    pub lib: Utf8PathBuf,
    pub env: Vec<(String, String)>,
    pub current_dir: Option<String>,
    pub data: ExpandMacroData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExpandMacroResponse {
    pub tree: FlatTree,
    pub span_data_table: Vec<u32>,
    pub tracked_env_vars: HashMap<Box<str>, Option<Box<str>>>,
    pub tracked_paths: HashSet<Box<str>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExpandMacroData {
    pub macro_body: FlatTree,
    pub macro_name: String,
    pub attributes: Option<FlatTree>,
    #[serde(default)]
    pub has_global_spans: ExpnGlobals,
    #[serde(default)]
    pub span_data_table: Vec<u32>,
}

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize)]
pub struct ExpnGlobals {
    pub def_site: usize,
    pub call_site: usize,
    pub mixed_site: usize,
}
