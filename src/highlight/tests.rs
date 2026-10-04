use super::languages::LANGS;
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
