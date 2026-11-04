//! The words each language reserves, the names of its own types, and the
//! words that declare a function, as the highlighter reads them.

use super::super::lex::Language;
use crate::code::listed;

/// Whether `language` reserves `word`: a keyword, or a word that stands for
/// a value of its own, such as `true`, `null` or `None`.
pub(super) fn reserved(language: Language, word: &str) -> bool {
    listed(reserved_by(language), word)
}

/// Whether `word` names one of `language`'s own types, as `u8`, `int` or
/// `string` do.
pub(super) fn typed(language: Language, word: &str) -> bool {
    listed(types_of(language), word)
}

/// Whether `language` writes `word` as a keyword after a `.`, where any other
/// word names a member: Rust's `.await`, Java's `String.class`, Scala's
/// `x.type`, Swift's `Int.self`.
pub(super) fn keyword_after_dot(language: Language, word: &str) -> bool {
    let after_dot = match language {
        Language::Rust => "await",
        Language::Java => "class new super this",
        Language::Scala => "super this type",
        Language::Swift => "init self",
        // These write no keyword after a dot.
        Language::C
        | Language::Go
        | Language::Kotlin
        | Language::CSharp
        | Language::Dart
        | Language::Script
        | Language::Python
        | Language::Shell
        | Language::Yaml
        | Language::Toml
        | Language::Json
        | Language::Markdown => "",
    };
    listed(after_dot, word)
}

/// Whether `word` declares a function in `language`, so the name after it
/// is the function's.
pub(super) fn declares(language: Language, word: &str) -> bool {
    match language {
        Language::Rust => word == "fn",
        Language::Go | Language::Swift => word == "func",
        Language::Python | Language::Scala => word == "def",
        Language::Kotlin => word == "fun",
        Language::Script | Language::Shell => word == "function",
        // These name a function after its type, or have no functions.
        Language::C
        | Language::Java
        | Language::CSharp
        | Language::Dart
        | Language::Yaml
        | Language::Toml
        | Language::Json
        | Language::Markdown => false,
    }
}

/// Whether `@name` is an attribute in `language`: a decorator or an
/// annotation.
pub(super) fn decorates(language: Language) -> bool {
    matches!(
        language,
        Language::Python
            | Language::Script
            | Language::Java
            | Language::Kotlin
            | Language::Scala
            | Language::Swift
            | Language::Dart
    )
}

/// The words `language` reserves, apart by spaces, or none when it has no list.
fn reserved_by(language: Language) -> &'static str {
    match language {
        Language::Rust => RUST,
        Language::C => C,
        Language::Java => JAVA,
        Language::Go => GO,
        Language::Kotlin => KOTLIN,
        Language::Scala => SCALA,
        Language::Swift => SWIFT,
        Language::CSharp => CSHARP,
        Language::Dart => DART,
        Language::Script => SCRIPT,
        Language::Python => PYTHON,
        Language::Shell => SHELL,
        Language::Yaml | Language::Toml | Language::Json => DATA,
        // Markdown reserves no word.
        Language::Markdown => "",
    }
}

/// The names of `language`'s own types, apart by spaces, or none when it has
/// no list.
fn types_of(language: Language) -> &'static str {
    match language {
        Language::Rust => RUST_TYPES,
        Language::C => C_TYPES,
        Language::Java => JAVA_TYPES,
        Language::Go => GO_TYPES,
        Language::CSharp => CSHARP_TYPES,
        Language::Dart => DART_TYPES,
        Language::Script => SCRIPT_TYPES,
        Language::Python => PYTHON_TYPES,
        // These have no types list of their own. Kotlin, Scala and Swift types open
        // with a capital, which the highlighter already reads as a type.
        Language::Kotlin
        | Language::Scala
        | Language::Swift
        | Language::Shell
        | Language::Yaml
        | Language::Toml
        | Language::Json
        | Language::Markdown => "",
    }
}

/// The words Rust reserves.
const RUST: &str = "as async await break const continue crate dyn else enum extern false fn for \
    if impl in let loop match mod move mut pub ref return self Self static struct super trait \
    true type union unsafe use where while yield";

/// Rust's own types.
const RUST_TYPES: &str = "bool char str u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize \
    f32 f64";

/// The words C and C++ reserve.
const C: &str = "auto break case catch class const constexpr continue default delete do else \
    enum explicit extern false for friend goto if inline namespace new noexcept nullptr \
    operator private protected public return sizeof static struct switch template this throw \
    true try typedef typename union using virtual volatile while";

/// C and C++'s own types.
const C_TYPES: &str = "void char short int long float double signed unsigned bool size_t \
    int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t";

/// The words Java reserves.
const JAVA: &str = "abstract assert break case catch class const continue default do else \
    enum extends false final finally for goto if implements import instanceof interface \
    native new null package permits private protected public record return sealed static \
    super switch synchronized this throw throws transient true try var volatile while yield";

/// Java's own types.
const JAVA_TYPES: &str = "boolean byte char short int long float double void";

/// The words Go reserves.
const GO: &str = "break case chan const continue default defer else fallthrough false for \
    func go goto if import interface iota map nil package range return select struct switch \
    true type var";

/// Go's own types.
const GO_TYPES: &str = "any bool byte complex64 complex128 error float32 float64 int int8 \
    int16 int32 int64 rune string uint uint8 uint16 uint32 uint64 uintptr";

/// The words Kotlin reserves.
const KOTLIN: &str = "as break by catch class companion constructor continue data do else \
    enum false final finally for fun if import in init inline interface internal is lateinit \
    null object open override package private protected public return sealed super suspend \
    this throw true try typealias val var vararg when while";

/// The words Scala reserves.
const SCALA: &str = "abstract case catch class def do else enum export extends extension \
    false final finally for given if implicit import lazy match new null object override \
    package private protected return sealed super then this throw trait true try type using \
    val var while with yield";

/// The words Swift reserves.
const SWIFT: &str = "any as associatedtype async await break case catch class continue \
    default defer deinit do else enum extension fallthrough false fileprivate for func guard \
    if import in init inout internal is let nil open operator private protocol public repeat \
    rethrows return self Self some static struct subscript super switch throw throws true try \
    typealias var where while";

/// The words C# reserves.
const CSHARP: &str = "abstract as async await base break case catch checked class const \
    continue default delegate do else enum event explicit extern false finally fixed for \
    foreach get goto if implicit in init interface internal is lock namespace new null \
    operator out override params private protected public readonly record ref return sealed \
    set sizeof stackalloc static struct switch this throw true try typeof unchecked unsafe \
    using var virtual volatile while yield";

/// C#'s own types.
const CSHARP_TYPES: &str = "bool byte sbyte char decimal double dynamic float int uint \
    long ulong short ushort nint nuint object string void";

/// The words Dart reserves.
const DART: &str = "abstract as assert async await break case catch class const continue \
    covariant default deferred do else enum export extends extension external factory false \
    final finally for get if implements import in interface is late library mixin new null \
    on operator part required rethrow return sealed set show static super switch sync this \
    throw true try typedef var while with yield";

/// Dart's own types.
const DART_TYPES: &str = "bool double dynamic int num void";

/// The words JavaScript and TypeScript reserve.
const SCRIPT: &str = "abstract as async await break case catch class const continue \
    debugger declare default delete do else enum export extends false finally for from \
    function get if implements import in instanceof interface keyof let module namespace new \
    null of private protected public readonly return satisfies set static super switch this \
    throw true try type typeof undefined var void while with yield";

/// JavaScript and TypeScript's own types.
const SCRIPT_TYPES: &str = "any bigint boolean never number object string symbol unknown";

/// The words Python reserves.
const PYTHON: &str = "and as assert async await break case class continue def del elif else \
    except False finally for from global if import in is lambda match None nonlocal not or \
    pass raise return True try while with yield";

/// Python's own types.
const PYTHON_TYPES: &str = "bool bytes dict float int list object set str tuple type";

/// The words sh, bash and zsh reserve.
const SHELL: &str = "alias break case continue declare do done elif else esac eval exec \
    exit export fi for function if in local readonly return select set shift source then \
    time trap unset until while";

/// The words YAML, TOML and JSON reserve.
const DATA: &str = "true false null";
