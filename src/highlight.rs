//! A small, dependency-free syntax highlighter for fenced code blocks.
//!
//! Each language is a table of keywords and comment/string rules fed to one
//! generic tokenizer. That is far from a real parser, but it is fast on
//! low-end machines and good enough to tell code apart at a glance. A block
//! whose language is missing or unknown is not highlighted at all.

use std::ops::Range;

/// What a piece of code is, for coloring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
}

/// The rules for one language.
#[derive(Debug)]
pub struct Lang {
    /// Display name, also the canonical fence tag.
    pub name: &'static str,
    aliases: &'static [&'static str],
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static [char],
    /// Python-style `"""` and `'''` strings.
    triple_quotes: bool,
    /// `'` only starts a string when it looks like a char literal (Rust lifetimes).
    char_literals: bool,
    /// Words starting with an uppercase letter are types.
    capitalized_types: bool,
    /// Keywords match regardless of case (SQL, assembly).
    case_insensitive: bool,
    /// `#word` at the start of a line is a directive (C preprocessor).
    preprocessor: bool,
    /// `$name` and `${name}` are variables (shells).
    dollar_vars: bool,
    /// The word after `<` or `</` is a tag (HTML, XML).
    tags: bool,
    /// A word or string followed by `:` or `=` is a key (JSON, YAML, TOML).
    keys: bool,
    /// The first word of a line is an instruction (assembly).
    first_word_keyword: bool,
    /// `name!` is a macro call (Rust).
    bang_macros: bool,
    /// A line ending in `:` opens a block (Python, YAML).
    pub colon_blocks: bool,
    /// Indent with a tab instead of four spaces (Go, Makefiles).
    pub tab_indent: bool,
}

impl Lang {
    const fn base(name: &'static str) -> Self {
        Self {
            name,
            aliases: &[],
            keywords: &[],
            types: &[],
            line_comments: &[],
            block_comment: None,
            quotes: &['"'],
            triple_quotes: false,
            char_literals: false,
            capitalized_types: false,
            case_insensitive: false,
            preprocessor: false,
            dollar_vars: false,
            tags: false,
            keys: false,
            first_word_keyword: false,
            bang_macros: false,
            colon_blocks: false,
            tab_indent: false,
        }
    }
}

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern", "for",
    "goto", "if", "inline", "register", "restrict", "return", "sizeof", "static", "struct",
    "switch", "typedef", "union", "volatile", "while", "NULL", "true", "false",
];
const C_TYPES: &[&str] = &[
    "char",
    "double",
    "float",
    "int",
    "long",
    "short",
    "signed",
    "unsigned",
    "void",
    "bool",
    "size_t",
    "ssize_t",
    "ptrdiff_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "uintptr_t",
    "intptr_t",
    "FILE",
    "wchar_t",
];
const CPP_KEYWORDS: &[&str] = &[
    "alignas",
    "alignof",
    "auto",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "consteval",
    "constexpr",
    "constinit",
    "const_cast",
    "continue",
    "co_await",
    "co_return",
    "co_yield",
    "decltype",
    "default",
    "delete",
    "do",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "final",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "nullptr",
    "operator",
    "override",
    "private",
    "protected",
    "public",
    "reinterpret_cast",
    "requires",
    "return",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typename",
    "union",
    "using",
    "virtual",
    "volatile",
    "while",
    "concept",
];
const CPP_TYPES: &[&str] = &[
    "bool",
    "char",
    "char8_t",
    "char16_t",
    "char32_t",
    "double",
    "float",
    "int",
    "long",
    "short",
    "signed",
    "unsigned",
    "void",
    "wchar_t",
    "size_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "std",
    "string",
    "vector",
    "map",
    "unordered_map",
    "unique_ptr",
    "shared_ptr",
    "optional",
    "array",
    "span",
    "string_view",
];

static LANGS: &[Lang] = &[
    Lang {
        aliases: &["rs"],
        keywords: &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while", "yield",
        ],
        types: &[
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
            "f32", "f64", "bool", "char", "str",
        ],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        char_literals: true,
        capitalized_types: true,
        bang_macros: true,
        ..Lang::base("rust")
    },
    Lang {
        aliases: &["py", "python3", "py3"],
        keywords: &[
            "False", "None", "True", "and", "as", "assert", "async", "await", "break", "case",
            "class", "continue", "def", "del", "elif", "else", "except", "finally", "for", "from",
            "global", "if", "import", "in", "is", "lambda", "match", "nonlocal", "not", "or",
            "pass", "raise", "return", "self", "try", "while", "with", "yield",
        ],
        types: &[
            "int",
            "float",
            "str",
            "bool",
            "bytes",
            "list",
            "dict",
            "set",
            "tuple",
            "object",
            "complex",
            "frozenset",
            "type",
        ],
        line_comments: &["#"],
        quotes: &['"', '\''],
        triple_quotes: true,
        capitalized_types: true,
        colon_blocks: true,
        ..Lang::base("python")
    },
    Lang {
        aliases: &["h"],
        keywords: C_KEYWORDS,
        types: C_TYPES,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\''],
        preprocessor: true,
        ..Lang::base("c")
    },
    Lang {
        aliases: &["c++", "cc", "cxx", "hpp", "hh", "hxx"],
        keywords: CPP_KEYWORDS,
        types: CPP_TYPES,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\''],
        preprocessor: true,
        capitalized_types: true,
        ..Lang::base("cpp")
    },
    Lang {
        aliases: &["cs", "c#"],
        keywords: &[
            "abstract",
            "as",
            "async",
            "await",
            "base",
            "break",
            "case",
            "catch",
            "checked",
            "class",
            "const",
            "continue",
            "default",
            "delegate",
            "do",
            "else",
            "enum",
            "event",
            "explicit",
            "extern",
            "false",
            "finally",
            "fixed",
            "for",
            "foreach",
            "get",
            "goto",
            "if",
            "implicit",
            "in",
            "interface",
            "internal",
            "is",
            "lock",
            "namespace",
            "new",
            "null",
            "operator",
            "out",
            "override",
            "params",
            "private",
            "protected",
            "public",
            "readonly",
            "record",
            "ref",
            "return",
            "sealed",
            "set",
            "sizeof",
            "static",
            "struct",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "unsafe",
            "using",
            "var",
            "virtual",
            "void",
            "volatile",
            "while",
            "yield",
        ],
        types: &[
            "bool", "byte", "char", "decimal", "double", "float", "int", "long", "object", "sbyte",
            "short", "string", "uint", "ulong", "ushort", "dynamic",
        ],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\''],
        preprocessor: true,
        capitalized_types: true,
        ..Lang::base("csharp")
    },
    Lang {
        aliases: &["kt", "kotlin"],
        keywords: &[
            "abstract",
            "assert",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "do",
            "else",
            "enum",
            "extends",
            "final",
            "finally",
            "for",
            "if",
            "implements",
            "import",
            "instanceof",
            "interface",
            "native",
            "new",
            "null",
            "package",
            "private",
            "protected",
            "public",
            "return",
            "static",
            "super",
            "switch",
            "synchronized",
            "this",
            "throw",
            "throws",
            "transient",
            "true",
            "false",
            "try",
            "var",
            "void",
            "volatile",
            "while",
            "record",
            "fun",
            "val",
            "when",
            "object",
            "is",
            "in",
        ],
        types: &[
            "boolean", "byte", "char", "double", "float", "int", "long", "short",
        ],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\''],
        capitalized_types: true,
        ..Lang::base("java")
    },
    Lang {
        aliases: &["js", "jsx", "mjs", "cjs", "ts", "tsx", "typescript", "node"],
        keywords: &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "from",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "let",
            "new",
            "null",
            "of",
            "return",
            "static",
            "super",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "undefined",
            "var",
            "void",
            "while",
            "with",
            "yield",
            "interface",
            "type",
            "enum",
            "implements",
            "private",
            "protected",
            "public",
            "readonly",
            "as",
            "declare",
            "namespace",
            "abstract",
        ],
        types: &[
            "string", "number", "boolean", "any", "unknown", "never", "object", "bigint", "symbol",
        ],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\'', '`'],
        capitalized_types: true,
        ..Lang::base("javascript")
    },
    Lang {
        aliases: &["golang"],
        keywords: &[
            "break",
            "case",
            "chan",
            "const",
            "continue",
            "default",
            "defer",
            "else",
            "fallthrough",
            "for",
            "func",
            "go",
            "goto",
            "if",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "return",
            "select",
            "struct",
            "switch",
            "type",
            "var",
            "true",
            "false",
            "nil",
            "iota",
        ],
        types: &[
            "bool",
            "byte",
            "complex64",
            "complex128",
            "error",
            "float32",
            "float64",
            "int",
            "int8",
            "int16",
            "int32",
            "int64",
            "rune",
            "string",
            "uint",
            "uint8",
            "uint16",
            "uint32",
            "uint64",
            "uintptr",
            "any",
        ],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\'', '`'],
        tab_indent: true,
        ..Lang::base("go")
    },
    Lang {
        aliases: &["hlsl", "vert", "frag", "wgsl", "shader"],
        keywords: &[
            "break",
            "case",
            "const",
            "continue",
            "default",
            "discard",
            "do",
            "else",
            "for",
            "if",
            "in",
            "inout",
            "out",
            "return",
            "struct",
            "switch",
            "uniform",
            "varying",
            "attribute",
            "layout",
            "precision",
            "highp",
            "mediump",
            "lowp",
            "while",
            "true",
            "false",
            "fn",
            "let",
            "var",
            "cbuffer",
            "register",
        ],
        types: &[
            "void",
            "bool",
            "int",
            "uint",
            "float",
            "double",
            "vec2",
            "vec3",
            "vec4",
            "ivec2",
            "ivec3",
            "ivec4",
            "uvec2",
            "uvec3",
            "uvec4",
            "mat2",
            "mat3",
            "mat4",
            "sampler2D",
            "samplerCube",
            "float2",
            "float3",
            "float4",
            "float4x4",
            "half",
            "Texture2D",
            "SamplerState",
            "f32",
            "i32",
            "u32",
        ],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        preprocessor: true,
        ..Lang::base("glsl")
    },
    Lang {
        aliases: &["jsonc", "json5"],
        keywords: &["true", "false", "null"],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keys: true,
        ..Lang::base("json")
    },
    Lang {
        aliases: &["ini", "cfg", "conf"],
        keywords: &["true", "false"],
        line_comments: &["#", ";"],
        quotes: &['"', '\''],
        triple_quotes: true,
        keys: true,
        ..Lang::base("toml")
    },
    Lang {
        aliases: &["yml"],
        keywords: &["true", "false", "null", "yes", "no", "on", "off"],
        line_comments: &["#"],
        quotes: &['"', '\''],
        keys: true,
        colon_blocks: true,
        ..Lang::base("yaml")
    },
    Lang {
        aliases: &["sh", "shell", "zsh", "console", "shellscript", "fish"],
        keywords: &[
            "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case",
            "esac", "in", "function", "return", "local", "export", "readonly", "declare", "source",
            "alias", "set", "unset", "shift", "exit", "break", "continue", "echo", "cd", "sudo",
        ],
        line_comments: &["#"],
        quotes: &['"', '\''],
        dollar_vars: true,
        ..Lang::base("bash")
    },
    Lang {
        aliases: &["ps1", "ps", "pwsh"],
        keywords: &[
            "begin",
            "break",
            "catch",
            "class",
            "continue",
            "data",
            "do",
            "dynamicparam",
            "else",
            "elseif",
            "end",
            "exit",
            "filter",
            "finally",
            "for",
            "foreach",
            "from",
            "function",
            "if",
            "in",
            "param",
            "process",
            "return",
            "switch",
            "throw",
            "trap",
            "try",
            "until",
            "using",
            "var",
            "while",
            "true",
            "false",
            "null",
        ],
        line_comments: &["#"],
        block_comment: Some(("<#", "#>")),
        quotes: &['"', '\''],
        dollar_vars: true,
        case_insensitive: true,
        ..Lang::base("powershell")
    },
    Lang {
        aliases: &["bat", "cmd"],
        keywords: &[
            "echo",
            "set",
            "if",
            "else",
            "for",
            "in",
            "do",
            "goto",
            "call",
            "exit",
            "not",
            "exist",
            "defined",
            "errorlevel",
            "setlocal",
            "endlocal",
            "rem",
            "pause",
            "shift",
        ],
        line_comments: &["::", "rem ", "REM "],
        case_insensitive: true,
        ..Lang::base("batch")
    },
    Lang {
        aliases: &["mysql", "postgres", "postgresql", "sqlite", "psql", "plsql"],
        keywords: &[
            "select",
            "from",
            "where",
            "and",
            "or",
            "not",
            "insert",
            "into",
            "values",
            "update",
            "set",
            "delete",
            "create",
            "table",
            "drop",
            "alter",
            "add",
            "column",
            "index",
            "on",
            "join",
            "left",
            "right",
            "inner",
            "outer",
            "full",
            "cross",
            "group",
            "by",
            "order",
            "having",
            "limit",
            "offset",
            "as",
            "distinct",
            "union",
            "all",
            "in",
            "is",
            "null",
            "like",
            "between",
            "exists",
            "case",
            "when",
            "then",
            "else",
            "end",
            "primary",
            "key",
            "foreign",
            "references",
            "default",
            "unique",
            "view",
            "with",
            "returning",
            "begin",
            "commit",
            "rollback",
            "transaction",
            "asc",
            "desc",
            "true",
            "false",
            "if",
            "count",
            "sum",
            "avg",
            "min",
            "max",
        ],
        types: &[
            "int",
            "integer",
            "bigint",
            "smallint",
            "text",
            "varchar",
            "char",
            "boolean",
            "date",
            "timestamp",
            "real",
            "float",
            "double",
            "decimal",
            "numeric",
            "blob",
            "serial",
            "uuid",
            "json",
            "jsonb",
        ],
        line_comments: &["--", "#"],
        block_comment: Some(("/*", "*/")),
        quotes: &['\'', '"', '`'],
        case_insensitive: true,
        ..Lang::base("sql")
    },
    Lang {
        aliases: &["xml", "htm", "xhtml", "svg", "vue", "svelte"],
        block_comment: Some(("<!--", "-->")),
        quotes: &['"', '\''],
        tags: true,
        ..Lang::base("html")
    },
    Lang {
        aliases: &["scss", "sass", "less"],
        keywords: &["!important", "inherit", "initial", "unset", "none", "auto"],
        line_comments: &[],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\''],
        keys: true,
        ..Lang::base("css")
    },
    Lang {
        aliases: &["luau"],
        keywords: &[
            "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto",
            "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until",
            "while", "self",
        ],
        block_comment: Some(("--[[", "]]")),
        line_comments: &["--"],
        quotes: &['"', '\''],
        ..Lang::base("lua")
    },
    Lang {
        aliases: &["asm", "nasm", "masm", "x86", "x86asm", "s", "gas", "x64"],
        keywords: &[
            "section", "segment", "global", "extern", "db", "dw", "dd", "dq", "resb", "resw",
            "resd", "resq", "equ", "times", "proc", "endp", "end", "byte", "word", "dword",
            "qword", "ptr", "include", "macro", "endm", "bits", "default", "rel",
        ],
        line_comments: &[";", "#", "//"],
        quotes: &['"', '\''],
        case_insensitive: true,
        first_word_keyword: true,
        ..Lang::base("assembly")
    },
    Lang {
        aliases: &["makefile", "mk"],
        keywords: &[
            "ifeq", "ifneq", "ifdef", "ifndef", "else", "endif", "include", "define", "endef",
            "export", "override", ".PHONY",
        ],
        line_comments: &["#"],
        quotes: &['"', '\''],
        dollar_vars: true,
        tab_indent: true,
        ..Lang::base("make")
    },
];

/// Assembly registers, colored as types.
fn is_register(word: &str) -> bool {
    const REGS: &[&str] = &[
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "rip", "eax", "ebx", "ecx", "edx",
        "esi", "edi", "ebp", "esp", "eip", "ax", "bx", "cx", "dx", "si", "di", "bp", "sp", "al",
        "ah", "bl", "bh", "cl", "ch", "dl", "dh", "sil", "dil", "cs", "ds", "es", "fs", "gs", "ss",
    ];
    let w = word.to_ascii_lowercase();
    if REGS.contains(&w.as_str()) {
        return true;
    }
    // r8..r15 with an optional d/w/b suffix, and the SIMD registers.
    let numbered = |prefix: &str| {
        w.strip_prefix(prefix).is_some_and(|rest| {
            let digits = rest.trim_end_matches(['d', 'w', 'b']);
            !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
        })
    };
    numbered("r") || numbered("xmm") || numbered("ymm") || numbered("zmm")
}

/// The language a fence's info string names (```` ```python ````), if it is one we know.
pub fn lang(info: &str) -> Option<&'static Lang> {
    // The first word; some writers use `{.python}` or `language-python`.
    let word = info
        .split(|c: char| c.is_whitespace() || c == ',' || c == '{' || c == '}')
        .find(|w| !w.is_empty())?;
    let word = word.trim_start_matches('.');
    let word = word.strip_prefix("language-").unwrap_or(word);
    let word = word.to_ascii_lowercase();
    LANGS
        .iter()
        .find(|l| l.name == word || l.aliases.contains(&word.as_str()))
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Splits `code` into colored ranges (byte offsets) covering all of it.
pub fn tokenize(lang: &Lang, code: &str) -> Vec<(Range<usize>, Token)> {
    let mut out: Vec<(Range<usize>, Token)> = Vec::new();
    let mut push = |range: Range<usize>, token: Token| {
        if range.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, t)) if *t == token && last.end == range.start => last.end = range.end,
            _ => out.push((range, token)),
        }
    };
    let bytes = code.as_bytes();
    let line_end = |from: usize| code[from..].find('\n').map_or(code.len(), |n| from + n);
    let mut line_start = true;
    let mut i = 0;
    while i < code.len() {
        let rest = &code[i..];
        let c = rest.chars().next().unwrap_or('\0');
        let at_line_start = line_start;
        if c == '\n' {
            line_start = true;
            push(i..i + 1, Token::Plain);
            i += 1;
            continue;
        }
        if !c.is_whitespace() {
            line_start = false;
        }

        // Comments.
        if let Some((open, close)) = lang.block_comment
            && rest.starts_with(open)
        {
            let end = rest[open.len()..]
                .find(close)
                .map_or(code.len(), |n| i + open.len() + n + close.len());
            push(i..end, Token::Comment);
            i = end;
            continue;
        }
        if lang.line_comments.iter().any(|p| {
            if lang.case_insensitive {
                rest.len() >= p.len()
                    && rest.is_char_boundary(p.len())
                    && rest[..p.len()].eq_ignore_ascii_case(p)
            } else {
                rest.starts_with(p)
            }
        }) && !(lang.dollar_vars && c == '#' && i > 0 && bytes[i - 1] == b'$')
        {
            let end = line_end(i);
            push(i..end, Token::Comment);
            i = end;
            continue;
        }

        // Directives and variables.
        if lang.preprocessor && c == '#' && at_line_start {
            let after = &rest[1..];
            let word = after.trim_start_matches([' ', '\t']);
            let len = word.find(|ch: char| !is_ident(ch)).unwrap_or(word.len());
            let word_end = i + 1 + (after.len() - word.len()) + len;
            push(i..word_end, Token::Keyword);
            i = word_end;
            continue;
        }
        if lang.dollar_vars && c == '$' {
            let after = &rest[1..];
            let len = if after.starts_with('{') || after.starts_with('(') {
                let close = if after.starts_with('{') { '}' } else { ')' };
                after.find(close).map_or(after.len(), |n| n + 1)
            } else {
                after.find(|ch: char| !is_ident(ch)).unwrap_or(after.len())
            };
            if len > 0 {
                push(i..i + 1 + len, Token::Type);
                i += 1 + len;
                continue;
            }
        }
        if lang.tags && c == '<' {
            let after = rest[1..].trim_start_matches(['/', '!', '?']);
            let skip = rest.len() - after.len();
            let len = after
                .find(|ch: char| !(is_ident(ch) || ch == '-' || ch == ':' || ch == '.'))
                .unwrap_or(after.len());
            push(i..i + skip, Token::Plain);
            push(i + skip..i + skip + len, Token::Keyword);
            i += skip + len;
            continue;
        }

        // Strings.
        if lang.triple_quotes && (rest.starts_with("\"\"\"") || rest.starts_with("'''")) {
            let delim = &rest[..3];
            let end = rest[3..].find(delim).map_or(code.len(), |n| i + 3 + n + 3);
            push(i..end, Token::String);
            i = end;
            continue;
        }
        if lang.quotes.contains(&c) || (lang.char_literals && c == '\'') {
            if lang.char_literals && c == '\'' && !looks_like_char_literal(rest) {
                // A lifetime or label: 'a, 'static.
                let len = 1 + rest[1..]
                    .find(|ch: char| !is_ident(ch))
                    .unwrap_or(rest.len() - 1);
                push(i..i + len, Token::Type);
                i += len;
                continue;
            }
            let end = string_end(rest, c, c == '`').map_or(code.len(), |n| i + n);
            let token = if lang.keys && is_key(code, end) {
                Token::Type
            } else {
                Token::String
            };
            push(i..end, token);
            i = end;
            continue;
        }

        // Numbers.
        let prev_ident = i > 0 && code[..i].chars().next_back().is_some_and(is_ident);
        if c.is_ascii_digit() && !prev_ident {
            let len = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                .unwrap_or(rest.len());
            push(i..i + len, Token::Number);
            i += len;
            continue;
        }

        // Words.
        if is_ident_start(c)
            || (c == '!' && lang.name == "css")
            || (c == '.' && lang.name == "make" && at_line_start)
        {
            let len = c.len_utf8()
                + rest[c.len_utf8()..]
                    .find(|ch: char| !(is_ident(ch) || (lang.name == "css" && ch == '-')))
                    .unwrap_or(rest.len() - c.len_utf8());
            let word = &rest[..len];
            let after = &rest[len..];
            let is_kw = if lang.case_insensitive {
                lang.keywords.iter().any(|k| k.eq_ignore_ascii_case(word))
            } else {
                lang.keywords.contains(&word)
            };
            let token =
                if is_kw || (lang.first_word_keyword && at_line_start && !after.starts_with(':')) {
                    Token::Keyword
                } else if (lang.keys && is_key(code, i + len))
                    || lang.types.contains(&word)
                    || (lang.case_insensitive
                        && lang.types.iter().any(|t| t.eq_ignore_ascii_case(word)))
                    || (lang.capitalized_types && word.starts_with(|ch: char| ch.is_uppercase()))
                    || (lang.name == "assembly" && is_register(word))
                {
                    Token::Type
                } else if after.trim_start_matches([' ', '\t']).starts_with('(')
                    || (lang.bang_macros && after.starts_with('!') && !after.starts_with("!="))
                {
                    Token::Function
                } else {
                    Token::Plain
                };
            let len = if token == Token::Function && lang.bang_macros && after.starts_with('!') {
                len + 1
            } else {
                len
            };
            push(i..i + len, token);
            i += len;
            continue;
        }

        push(i..i + c.len_utf8(), Token::Plain);
        i += c.len_utf8();
    }
    out
}

/// Byte length of the string starting at the quote `rest[0]`, closing quote included.
/// Ordinary strings end at the line; `multiline` ones (JS templates) may span lines.
fn string_end(rest: &str, quote: char, multiline: bool) -> Option<usize> {
    let mut escaped = false;
    for (n, ch) in rest.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(n + ch.len_utf8());
        } else if ch == '\n' && !multiline {
            return Some(n);
        }
    }
    None
}

/// `'x'` or `'\n'`, as opposed to a Rust lifetime like `'a`.
fn looks_like_char_literal(rest: &str) -> bool {
    let mut chars = rest.chars().skip(1);
    match chars.next() {
        Some('\\') => true,
        Some(_) => chars.next() == Some('\''),
        None => false,
    }
}

/// True when the text after `end` (spaces skipped) is `:` or `=`, making what precedes it a key.
fn is_key(code: &str, end: usize) -> bool {
    let after = code[end..].trim_start_matches([' ', '\t']);
    (after.starts_with(':') && !after.starts_with("::"))
        || (after.starts_with('=') && !after.starts_with("=="))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lang_name: &str, code: &str) -> Vec<(String, Token)> {
        let lang = lang(lang_name).expect("known language");
        tokenize(lang, code)
            .into_iter()
            .filter(|(_, t)| *t != Token::Plain)
            .map(|(r, t)| (code[r].to_owned(), t))
            .collect()
    }

    #[test]
    fn languages_resolve_from_fence_info() {
        assert_eq!(lang("python").map(|l| l.name), Some("python"));
        assert_eq!(lang("py title=x").map(|l| l.name), Some("python"));
        assert_eq!(lang("C++").map(|l| l.name), Some("cpp"));
        assert_eq!(lang("{.rust}").map(|l| l.name), Some("rust"));
        assert_eq!(lang("language-ts").map(|l| l.name), Some("javascript"));
        assert!(lang("").is_none());
        assert!(lang("klingon").is_none());
    }

    #[test]
    fn tokens_cover_the_whole_input() {
        let code = "fn main() {\n    let s = \"hé\"; // ñ\n}\n";
        for l in LANGS {
            let tokens = tokenize(l, code);
            let mut at = 0;
            for (r, _) in &tokens {
                assert_eq!(r.start, at, "gap in {}", l.name);
                at = r.end;
            }
            assert_eq!(at, code.len(), "{} stops early", l.name);
        }
    }

    #[test]
    fn python() {
        let k = kinds(
            "python",
            "def f(x):\n    return 'a' # done\n\"\"\"doc\nmore\"\"\"\n",
        );
        assert!(k.contains(&("def".into(), Token::Keyword)));
        assert!(k.contains(&("f".into(), Token::Function)));
        assert!(k.contains(&("'a'".into(), Token::String)));
        assert!(k.contains(&("# done".into(), Token::Comment)));
        assert!(k.contains(&("\"\"\"doc\nmore\"\"\"".into(), Token::String)));
    }

    #[test]
    fn cpp() {
        let k = kinds(
            "cpp",
            "#include <vector>\nint main() { /* x */ return 0x1F; }",
        );
        assert!(k.contains(&("#include".into(), Token::Keyword)));
        assert!(k.contains(&("int".into(), Token::Type)));
        assert!(k.contains(&("main".into(), Token::Function)));
        assert!(k.contains(&("/* x */".into(), Token::Comment)));
        assert!(k.contains(&("0x1F".into(), Token::Number)));
    }

    #[test]
    fn rust_lifetimes_and_macros() {
        let k = kinds("rust", "fn f<'a>(s: &'a str) { println!(\"{}\", 'c'); }");
        assert!(k.contains(&("'a".into(), Token::Type)));
        assert!(k.contains(&("println!".into(), Token::Function)));
        assert!(k.contains(&("'c'".into(), Token::String)));
    }

    #[test]
    fn json_keys() {
        let k = kinds("json", "{\"name\": \"nt\", \"n\": 3, \"ok\": true}");
        assert!(k.contains(&("\"name\"".into(), Token::Type)));
        assert!(k.contains(&("\"nt\"".into(), Token::String)));
        assert!(k.contains(&("3".into(), Token::Number)));
        assert!(k.contains(&("true".into(), Token::Keyword)));
    }

    #[test]
    fn shell_and_assembly() {
        let k = kinds("bash", "echo \"$HOME\" $USER # hi");
        assert!(k.contains(&("$USER".into(), Token::Type)));
        assert!(k.contains(&("# hi".into(), Token::Comment)));
        let k = kinds("nasm", "    mov rax, 1 ; exit\n");
        assert!(k.contains(&("mov".into(), Token::Keyword)));
        assert!(k.contains(&("rax".into(), Token::Type)));
        assert!(k.contains(&("; exit".into(), Token::Comment)));
    }

    #[test]
    fn unterminated_things_stop_at_the_end() {
        let k = kinds("c", "/* open\nstill");
        assert_eq!(k, vec![("/* open\nstill".into(), Token::Comment)]);
        let k = kinds("python", "x = 'abc\ny");
        assert!(k.contains(&("'abc".into(), Token::String)));
    }
}
