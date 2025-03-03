//! This module contains free-standing functions for creating AST fragments out
//! of smaller pieces.
//!
//! Note that all functions here intended to be stupid constructors, which just
//! assemble a finish node from immediate children. If you want to do something
//! smarter than that, it belongs to the `ext` submodule.
//!
//! Keep in mind that `from_text` functions should be kept private. The public
//! API should require to assemble every node piecewise. The trick of
//! `parse(format!())` we use internally is an implementation detail -- long
//! term, it will be replaced with direct tree manipulation.
use itertools::Itertools;
use stdx::{format_to, never};

use crate::{ast, AstNode, SourceFile, SyntaxKind, SyntaxToken};

/// While the parent module defines basic atomic "constructors", the `ext`
/// module defines shortcuts for common things.
///
/// It's named `ext` rather than `shortcuts` just to keep it short.
pub mod ext {
    use super::*;

    pub fn simple_ident_pat(name: ast::Name) -> ast::IdentPat {
        return from_text(&name.text());

        fn from_text(text: &str) -> ast::IdentPat {
            ast_from_text(&format!("fn f({}: ())", text))
        }
    }
    pub fn ident_path(ident: &str) -> ast::Path {
        path_unqualified(path_segment(name_ref(ident)))
    }

    pub fn expr_unreachable() -> ast::Expr {
        expr_from_text("unreachable!()")
    }
    pub fn expr_todo() -> ast::Expr {
        expr_from_text("todo!()")
    }
    pub fn empty_block_expr() -> ast::BlockExpr {
        block_expr(None, None)
    }

    pub fn ty_bool() -> ast::Type {
        ty_path(ident_path("bool"))
    }
    pub fn ty_option(t: ast::Type) -> ast::Type {
        ty_from_text(&format!("Option<{}>", t))
    }
    pub fn ty_result(t: ast::Type, e: ast::Type) -> ast::Type {
        ty_from_text(&format!("Result<{}, {}>", t, e))
    }
}

pub fn name(text: &str) -> ast::Name {
    ast_from_text(&format!("mod {}{};", raw_ident_esc(text), text))
}
pub fn name_ref(text: &str) -> ast::NameRef {
    ast_from_text(&format!("fn f() {{ {}{}; }}", raw_ident_esc(text), text))
}
fn raw_ident_esc(ident: &str) -> &'static str {
    let is_keyword = parser::SyntaxKind::from_keyword(ident).is_some();
    if is_keyword && !matches!(ident, "self" | "crate" | "super" | "Self") {
        "r#"
    } else {
        ""
    }
}

pub fn lifetime(text: &str) -> ast::Lifetime {
    let mut text = text;
    let tmp;
    if never!(!text.starts_with('\'')) {
        tmp = format!("'{}", text);
        text = &tmp;
    }
    ast_from_text(&format!("fn f<{}>() {{ }}", text))
}

// FIXME: replace stringly-typed constructor with a family of typed ctors, a-la
// `expr_xxx`.
pub fn ty(text: &str) -> ast::Type {
    ty_from_text(text)
}
pub fn ty_unit() -> ast::Type {
    ty_from_text("()")
}
pub fn ty_tuple(types: impl IntoIterator<Item = ast::Type>) -> ast::Type {
    let mut count: usize = 0;
    let mut contents = types.into_iter().inspect(|_| count += 1).join(", ");
    if count == 1 {
        contents.push(',');
    }

    ty_from_text(&format!("({})", contents))
}
pub fn ty_ref(target: ast::Type, exclusive: bool) -> ast::Type {
    ty_from_text(&if exclusive { format!("&mut {}", target) } else { format!("&{}", target) })
}
pub fn ty_path(path: ast::Path) -> ast::Type {
    ty_from_text(&path.to_string())
}
fn ty_from_text(text: &str) -> ast::Type {
    ast_from_text(&format!("type _T = {};", text))
}

pub fn assoc_item_list() -> ast::AssocItemList {
    ast_from_text("impl C for D {}")
}

pub fn impl_trait(trait_: ast::Path, ty: ast::Path) -> ast::Impl {
    ast_from_text(&format!("impl {} for {} {{}}", trait_, ty))
}

pub(crate) fn generic_arg_list() -> ast::GenericArgList {
    ast_from_text("const S: T<> = ();")
}

pub fn path_segment(name_ref: ast::NameRef) -> ast::PathSegment {
    ast_from_text(&format!("use {};", name_ref))
}

pub fn path_segment_self() -> ast::PathSegment {
    ast_from_text("use self;")
}

pub fn path_segment_super() -> ast::PathSegment {
    ast_from_text("use super;")
}

pub fn path_segment_crate() -> ast::PathSegment {
    ast_from_text("use crate;")
}

pub fn path_unqualified(segment: ast::PathSegment) -> ast::Path {
    ast_from_text(&format!("use {}", segment))
}

pub fn path_qualified(qual: ast::Path, segment: ast::PathSegment) -> ast::Path {
    ast_from_text(&format!("{}::{}", qual, segment))
}
// FIXME: path concatenation operation doesn't make sense as AST op.
pub fn path_concat(first: ast::Path, second: ast::Path) -> ast::Path {
    ast_from_text(&format!("{}::{}", first, second))
}

pub fn path_from_segments(
    segments: impl IntoIterator<Item = ast::PathSegment>,
    is_abs: bool,
) -> ast::Path {
    let segments = segments.into_iter().map(|it| it.syntax().clone()).join("::");
    ast_from_text(&if is_abs {
        format!("use ::{};", segments)
    } else {
        format!("use {};", segments)
    })
}
// FIXME: should not be pub
pub fn path_from_text(text: &str) -> ast::Path {
    ast_from_text(&format!("fn main() {{ let test = {}; }}", text))
}

pub fn use_tree_glob() -> ast::UseTree {
    ast_from_text("use *;")
}
pub fn use_tree(
    path: ast::Path,
    use_tree_list: Option<ast::UseTreeList>,
    alias: Option<ast::Rename>,
    add_star: bool,
) -> ast::UseTree {
    let mut buf = "use ".to_string();
    buf += &path.syntax().to_string();
    if let Some(use_tree_list) = use_tree_list {
        format_to!(buf, "::{}", use_tree_list);
    }
    if add_star {
        buf += "::*";
    }

    if let Some(alias) = alias {
        format_to!(buf, " {}", alias);
    }
    ast_from_text(&buf)
}

pub fn use_tree_list(use_trees: impl IntoIterator<Item = ast::UseTree>) -> ast::UseTreeList {
    let use_trees = use_trees.into_iter().map(|it| it.syntax().clone()).join(", ");
    ast_from_text(&format!("use {{{}}};", use_trees))
}

pub fn use_(visibility: Option<ast::Visibility>, use_tree: ast::UseTree) -> ast::Use {
    let visibility = match visibility {
        None => String::new(),
        Some(it) => format!("{} ", it),
    };
    ast_from_text(&format!("{}use {};", visibility, use_tree))
}

pub fn record_expr(path: ast::Path, fields: ast::RecordExprFieldList) -> ast::RecordExpr {
    ast_from_text(&format!("fn f() {{ {} {} }}", path, fields))
}

pub fn record_expr_field_list(
    fields: impl IntoIterator<Item = ast::RecordExprField>,
) -> ast::RecordExprFieldList {
    let fields = fields.into_iter().join(", ");
    ast_from_text(&format!("fn f() {{ S {{ {} }} }}", fields))
}

pub fn record_expr_field(name: ast::NameRef, expr: Option<ast::Expr>) -> ast::RecordExprField {
    return match expr {
        Some(expr) => from_text(&format!("{}: {}", name, expr)),
        None => from_text(&name.to_string()),
    };

    fn from_text(text: &str) -> ast::RecordExprField {
        ast_from_text(&format!("fn f() {{ S {{ {}, }} }}", text))
    }
}

pub fn record_field(
    visibility: Option<ast::Visibility>,
    name: ast::Name,
    ty: ast::Type,
) -> ast::RecordField {
    let visibility = match visibility {
        None => String::new(),
        Some(it) => format!("{} ", it),
    };
    ast_from_text(&format!("struct S {{ {}{}: {}, }}", visibility, name, ty))
}

pub fn block_expr(
    stmts: impl IntoIterator<Item = ast::Stmt>,
    tail_expr: Option<ast::Expr>,
) -> ast::BlockExpr {
    let mut buf = "{\n".to_string();
    for stmt in stmts.into_iter() {
        format_to!(buf, "    {}\n", stmt);
    }
    if let Some(tail_expr) = tail_expr {
        format_to!(buf, "    {}\n", tail_expr)
    }
    buf += "}";
    ast_from_text(&format!("fn f() {}", buf))
}

pub fn expr_unit() -> ast::Expr {
    expr_from_text("()")
}
pub fn expr_literal(text: &str) -> ast::Literal {
    assert_eq!(text.trim(), text);
    ast_from_text(&format!("fn f() {{ let _ = {}; }}", text))
}

pub fn expr_empty_block() -> ast::Expr {
    expr_from_text("{}")
}
pub fn expr_path(path: ast::Path) -> ast::Expr {
    expr_from_text(&path.to_string())
}
pub fn expr_continue() -> ast::Expr {
    expr_from_text("continue")
}
pub fn expr_break(expr: Option<ast::Expr>) -> ast::Expr {
    match expr {
        Some(expr) => expr_from_text(&format!("break {}", expr)),
        None => expr_from_text("break"),
    }
}
pub fn expr_return(expr: Option<ast::Expr>) -> ast::Expr {
    match expr {
        Some(expr) => expr_from_text(&format!("return {}", expr)),
        None => expr_from_text("return"),
    }
}
pub fn expr_try(expr: ast::Expr) -> ast::Expr {
    expr_from_text(&format!("{}?", expr))
}
pub fn expr_match(expr: ast::Expr, match_arm_list: ast::MatchArmList) -> ast::Expr {
    expr_from_text(&format!("match {} {}", expr, match_arm_list))
}
pub fn expr_if(
    condition: ast::Condition,
    then_branch: ast::BlockExpr,
    else_branch: Option<ast::ElseBranch>,
) -> ast::Expr {
    let else_branch = match else_branch {
        Some(ast::ElseBranch::Block(block)) => format!("else {}", block),
        Some(ast::ElseBranch::IfExpr(if_expr)) => format!("else {}", if_expr),
        None => String::new(),
    };
    expr_from_text(&format!("if {} {} {}", condition, then_branch, else_branch))
}
pub fn expr_for_loop(pat: ast::Pat, expr: ast::Expr, block: ast::BlockExpr) -> ast::Expr {
    expr_from_text(&format!("for {} in {} {}", pat, expr, block))
}
pub fn expr_prefix(op: SyntaxKind, expr: ast::Expr) -> ast::Expr {
    let token = token(op);
    expr_from_text(&format!("{}{}", token, expr))
}
pub fn expr_call(f: ast::Expr, arg_list: ast::ArgList) -> ast::Expr {
    expr_from_text(&format!("{}{}", f, arg_list))
}
pub fn expr_method_call(receiver: ast::Expr, method: &str, arg_list: ast::ArgList) -> ast::Expr {
    expr_from_text(&format!("{}.{}{}", receiver, method, arg_list))
}
pub fn expr_ref(expr: ast::Expr, exclusive: bool) -> ast::Expr {
    expr_from_text(&if exclusive { format!("&mut {}", expr) } else { format!("&{}", expr) })
}
pub fn expr_paren(expr: ast::Expr) -> ast::Expr {
    expr_from_text(&format!("({})", expr))
}
pub fn expr_tuple(elements: impl IntoIterator<Item = ast::Expr>) -> ast::Expr {
    let expr = elements.into_iter().format(", ");
    expr_from_text(&format!("({})", expr))
}
pub fn expr_assignment(lhs: ast::Expr, rhs: ast::Expr) -> ast::Expr {
    expr_from_text(&format!("{} = {}", lhs, rhs))
}
fn expr_from_text(text: &str) -> ast::Expr {
    ast_from_text(&format!("const C: () = {};", text))
}

pub fn condition(expr: ast::Expr, pattern: Option<ast::Pat>) -> ast::Condition {
    match pattern {
        None => ast_from_text(&format!("const _: () = while {} {{}};", expr)),
        Some(pattern) => {
            ast_from_text(&format!("const _: () = while let {} = {} {{}};", pattern, expr))
        }
    }
}

pub fn arg_list(args: impl IntoIterator<Item = ast::Expr>) -> ast::ArgList {
    ast_from_text(&format!("fn main() {{ ()({}) }}", args.into_iter().format(", ")))
}

pub fn ident_pat(ref_: bool, mut_: bool, name: ast::Name) -> ast::IdentPat {
    let mut s = String::from("fn f(");
    if ref_ {
        s.push_str("ref ");
    }
    if mut_ {
        s.push_str("mut ");
    }
    format_to!(s, "{}", name);
    s.push_str(": ())");
    ast_from_text(&s)
}

pub fn wildcard_pat() -> ast::WildcardPat {
    return from_text("_");

    fn from_text(text: &str) -> ast::WildcardPat {
        ast_from_text(&format!("fn f({}: ())", text))
    }
}

pub fn literal_pat(lit: &str) -> ast::LiteralPat {
    return from_text(lit);

    fn from_text(text: &str) -> ast::LiteralPat {
        ast_from_text(&format!("fn f() {{ match x {{ {} => {{}} }} }}", text))
    }
}

/// Creates a tuple of patterns from an iterator of patterns.
///
/// Invariant: `pats` must be length > 0
pub fn tuple_pat(pats: impl IntoIterator<Item = ast::Pat>) -> ast::TuplePat {
    let mut count: usize = 0;
    let mut pats_str = pats.into_iter().inspect(|_| count += 1).join(", ");
    if count == 1 {
        pats_str.push(',');
    }
    return from_text(&format!("({})", pats_str));

    fn from_text(text: &str) -> ast::TuplePat {
        ast_from_text(&format!("fn f({}: ())", text))
    }
}

pub fn tuple_struct_pat(
    path: ast::Path,
    pats: impl IntoIterator<Item = ast::Pat>,
) -> ast::TupleStructPat {
    let pats_str = pats.into_iter().join(", ");
    return from_text(&format!("{}({})", path, pats_str));

    fn from_text(text: &str) -> ast::TupleStructPat {
        ast_from_text(&format!("fn f({}: ())", text))
    }
}

pub fn record_pat(path: ast::Path, pats: impl IntoIterator<Item = ast::Pat>) -> ast::RecordPat {
    let pats_str = pats.into_iter().join(", ");
    return from_text(&format!("{} {{ {} }}", path, pats_str));

    fn from_text(text: &str) -> ast::RecordPat {
        ast_from_text(&format!("fn f({}: ())", text))
    }
}

pub fn record_pat_with_fields(path: ast::Path, fields: ast::RecordPatFieldList) -> ast::RecordPat {
    ast_from_text(&format!("fn f({} {}: ()))", path, fields))
}

pub fn record_pat_field_list(
    fields: impl IntoIterator<Item = ast::RecordPatField>,
) -> ast::RecordPatFieldList {
    let fields = fields.into_iter().join(", ");
    ast_from_text(&format!("fn f(S {{ {} }}: ()))", fields))
}

pub fn record_pat_field(name_ref: ast::NameRef, pat: ast::Pat) -> ast::RecordPatField {
    ast_from_text(&format!("fn f(S {{ {}: {} }}: ()))", name_ref, pat))
}

/// Returns a `BindPat` if the path has just one segment, a `PathPat` otherwise.
pub fn path_pat(path: ast::Path) -> ast::Pat {
    return from_text(&path.to_string());
    fn from_text(text: &str) -> ast::Pat {
        ast_from_text(&format!("fn f({}: ())", text))
    }
}

pub fn match_arm(
    pats: impl IntoIterator<Item = ast::Pat>,
    guard: Option<ast::Expr>,
    expr: ast::Expr,
) -> ast::MatchArm {
    let pats_str = pats.into_iter().join(" | ");
    return match guard {
        Some(guard) => from_text(&format!("{} if {} => {}", pats_str, guard, expr)),
        None => from_text(&format!("{} => {}", pats_str, expr)),
    };

    fn from_text(text: &str) -> ast::MatchArm {
        ast_from_text(&format!("fn f() {{ match () {{{}}} }}", text))
    }
}

pub fn match_arm_with_guard(
    pats: impl IntoIterator<Item = ast::Pat>,
    guard: ast::Expr,
    expr: ast::Expr,
) -> ast::MatchArm {
    let pats_str = pats.into_iter().join(" | ");
    return from_text(&format!("{} if {} => {}", pats_str, guard, expr));

    fn from_text(text: &str) -> ast::MatchArm {
        ast_from_text(&format!("fn f() {{ match () {{{}}} }}", text))
    }
}

pub fn match_arm_list(arms: impl IntoIterator<Item = ast::MatchArm>) -> ast::MatchArmList {
    let arms_str = arms
        .into_iter()
        .map(|arm| {
            let needs_comma = arm.expr().map_or(true, |it| !it.is_block_like());
            let comma = if needs_comma { "," } else { "" };
            format!("    {}{}\n", arm.syntax(), comma)
        })
        .collect::<String>();
    return from_text(&arms_str);

    fn from_text(text: &str) -> ast::MatchArmList {
        ast_from_text(&format!("fn f() {{ match () {{\n{}}} }}", text))
    }
}

pub fn where_pred(
    path: ast::Path,
    bounds: impl IntoIterator<Item = ast::TypeBound>,
) -> ast::WherePred {
    let bounds = bounds.into_iter().join(" + ");
    return from_text(&format!("{}: {}", path, bounds));

    fn from_text(text: &str) -> ast::WherePred {
        ast_from_text(&format!("fn f() where {} {{ }}", text))
    }
}

pub fn where_clause(preds: impl IntoIterator<Item = ast::WherePred>) -> ast::WhereClause {
    let preds = preds.into_iter().join(", ");
    return from_text(preds.as_str());

    fn from_text(text: &str) -> ast::WhereClause {
        ast_from_text(&format!("fn f() where {} {{ }}", text))
    }
}

pub fn let_stmt(pattern: ast::Pat, initializer: Option<ast::Expr>) -> ast::LetStmt {
    let text = match initializer {
        Some(it) => format!("let {} = {};", pattern, it),
        None => format!("let {};", pattern),
    };
    ast_from_text(&format!("fn f() {{ {} }}", text))
}
pub fn expr_stmt(expr: ast::Expr) -> ast::ExprStmt {
    let semi = if expr.is_block_like() { "" } else { ";" };
    ast_from_text(&format!("fn f() {{ {}{} (); }}", expr, semi))
}

pub fn param(pat: ast::Pat, ty: ast::Type) -> ast::Param {
    ast_from_text(&format!("fn f({}: {}) {{ }}", pat, ty))
}

pub fn ret_type(ty: ast::Type) -> ast::RetType {
    ast_from_text(&format!("fn f() -> {} {{ }}", ty))
}

pub fn param_list(
    self_param: Option<ast::SelfParam>,
    pats: impl IntoIterator<Item = ast::Param>,
) -> ast::ParamList {
    let args = pats.into_iter().join(", ");
    let list = match self_param {
        Some(self_param) if args.is_empty() => format!("fn f({}) {{ }}", self_param),
        Some(self_param) => format!("fn f({}, {}) {{ }}", self_param, args),
        None => format!("fn f({}) {{ }}", args),
    };
    ast_from_text(&list)
}

pub fn type_param(name: ast::Name, ty: Option<ast::TypeBoundList>) -> ast::TypeParam {
    let bound = match ty {
        Some(it) => format!(": {}", it),
        None => String::new(),
    };
    ast_from_text(&format!("fn f<{}{}>() {{ }}", name, bound))
}

pub fn lifetime_param(lifetime: ast::Lifetime) -> ast::LifetimeParam {
    ast_from_text(&format!("fn f<{}>() {{ }}", lifetime))
}

pub fn generic_param_list(
    pats: impl IntoIterator<Item = ast::GenericParam>,
) -> ast::GenericParamList {
    let args = pats.into_iter().join(", ");
    ast_from_text(&format!("fn f<{}>() {{ }}", args))
}

pub fn visibility_pub_crate() -> ast::Visibility {
    ast_from_text("pub(crate) struct S")
}

pub fn visibility_pub() -> ast::Visibility {
    ast_from_text("pub struct S")
}

pub fn tuple_field_list(fields: impl IntoIterator<Item = ast::TupleField>) -> ast::TupleFieldList {
    let fields = fields.into_iter().join(", ");
    ast_from_text(&format!("struct f({});", fields))
}

pub fn record_field_list(
    fields: impl IntoIterator<Item = ast::RecordField>,
) -> ast::RecordFieldList {
    let fields = fields.into_iter().join(", ");
    ast_from_text(&format!("struct f {{ {} }}", fields))
}

pub fn tuple_field(visibility: Option<ast::Visibility>, ty: ast::Type) -> ast::TupleField {
    let visibility = match visibility {
        None => String::new(),
        Some(it) => format!("{} ", it),
    };
    ast_from_text(&format!("struct f({}{});", visibility, ty))
}

pub fn variant(name: ast::Name, field_list: Option<ast::FieldList>) -> ast::Variant {
    let field_list = match field_list {
        None => String::new(),
        Some(it) => format!("{}", it),
    };
    ast_from_text(&format!("enum f {{ {}{} }}", name, field_list))
}

pub fn fn_(
    visibility: Option<ast::Visibility>,
    fn_name: ast::Name,
    type_params: Option<ast::GenericParamList>,
    params: ast::ParamList,
    body: ast::BlockExpr,
    ret_type: Option<ast::RetType>,
) -> ast::Fn {
    let type_params =
        if let Some(type_params) = type_params { format!("<{}>", type_params) } else { "".into() };
    let ret_type = if let Some(ret_type) = ret_type { format!("{} ", ret_type) } else { "".into() };
    let visibility = match visibility {
        None => String::new(),
        Some(it) => format!("{} ", it),
    };

    ast_from_text(&format!(
        "{}fn {}{}{} {}{}",
        visibility, fn_name, type_params, params, ret_type, body
    ))
}

pub fn struct_(
    visibility: Option<ast::Visibility>,
    strukt_name: ast::Name,
    generic_param_list: Option<ast::GenericParamList>,
    field_list: ast::FieldList,
) -> ast::Struct {
    let semicolon = if matches!(field_list, ast::FieldList::TupleFieldList(_)) { ";" } else { "" };
    let type_params = generic_param_list.map_or_else(String::new, |it| it.to_string());
    let visibility = match visibility {
        None => String::new(),
        Some(it) => format!("{} ", it),
    };

    ast_from_text(&format!(
        "{}struct {}{}{}{}",
        visibility, strukt_name, type_params, field_list, semicolon
    ))
}

fn ast_from_text<N: AstNode>(text: &str) -> N {
    let parse = SourceFile::parse(text);
    let node = match parse.tree().syntax().descendants().find_map(N::cast) {
        Some(it) => it,
        None => {
            panic!("Failed to make ast node `{}` from text {}", std::any::type_name::<N>(), text)
        }
    };
    let node = node.clone_subtree();
    assert_eq!(node.syntax().text_range().start(), 0.into());
    node
}

pub fn token(kind: SyntaxKind) -> SyntaxToken {
    tokens::SOURCE_FILE
        .tree()
        .syntax()
        .clone_for_update()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == kind)
        .unwrap_or_else(|| panic!("unhandled token: {:?}", kind))
}

pub mod tokens {
    use once_cell::sync::Lazy;

    use crate::{ast, AstNode, Parse, SourceFile, SyntaxKind::*, SyntaxToken};

    pub(super) static SOURCE_FILE: Lazy<Parse<SourceFile>> = Lazy::new(|| {
        SourceFile::parse(
            "const C: <()>::Item = (1 != 1, 2 == 2, 3 < 3, 4 <= 4, 5 > 5, 6 >= 6, !true, *p)\n;\n\n",
        )
    });

    pub fn single_space() -> SyntaxToken {
        SOURCE_FILE
            .tree()
            .syntax()
            .clone_for_update()
            .descendants_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == WHITESPACE && it.text() == " ")
            .unwrap()
    }

    pub fn whitespace(text: &str) -> SyntaxToken {
        assert!(text.trim().is_empty());
        let sf = SourceFile::parse(text).ok().unwrap();
        sf.syntax().clone_for_update().first_child_or_token().unwrap().into_token().unwrap()
    }

    pub fn doc_comment(text: &str) -> SyntaxToken {
        assert!(!text.trim().is_empty());
        let sf = SourceFile::parse(text).ok().unwrap();
        sf.syntax().first_child_or_token().unwrap().into_token().unwrap()
    }

    pub fn literal(text: &str) -> SyntaxToken {
        assert_eq!(text.trim(), text);
        let lit: ast::Literal = super::ast_from_text(&format!("fn f() {{ let _ = {}; }}", text));
        lit.syntax().first_child_or_token().unwrap().into_token().unwrap()
    }

    pub fn single_newline() -> SyntaxToken {
        let res = SOURCE_FILE
            .tree()
            .syntax()
            .clone_for_update()
            .descendants_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == WHITESPACE && it.text() == "\n")
            .unwrap();
        res.detach();
        res
    }

    pub fn blank_line() -> SyntaxToken {
        SOURCE_FILE
            .tree()
            .syntax()
            .clone_for_update()
            .descendants_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == WHITESPACE && it.text() == "\n\n")
            .unwrap()
    }

    pub struct WsBuilder(SourceFile);

    impl WsBuilder {
        pub fn new(text: &str) -> WsBuilder {
            WsBuilder(SourceFile::parse(text).ok().unwrap())
        }
        pub fn ws(&self) -> SyntaxToken {
            self.0.syntax().first_child_or_token().unwrap().into_token().unwrap()
        }
    }
}
