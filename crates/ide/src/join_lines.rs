use std::convert::TryFrom;

use ide_assists::utils::extract_trivial_expression;
use itertools::Itertools;
use syntax::{
    algo::non_trivia_sibling,
    ast::{self, AstNode, AstToken, IsString},
    Direction, NodeOrToken, SourceFile, SyntaxElement,
    SyntaxKind::{self, USE_TREE, WHITESPACE},
    SyntaxNode, SyntaxToken, TextRange, TextSize, T,
};

use text_edit::{TextEdit, TextEditBuilder};

// Feature: Join Lines
//
// Join selected lines into one, smartly fixing up whitespace, trailing commas, and braces.
//
// |===
// | Editor  | Action Name
//
// | VS Code | **Rust Analyzer: Join lines**
// |===
//
// image::https://user-images.githubusercontent.com/48062697/113020661-b6922200-917a-11eb-87c4-b75acc028f11.gif[]
pub(crate) fn join_lines(file: &SourceFile, range: TextRange) -> TextEdit {
    let range = if range.is_empty() {
        let syntax = file.syntax();
        let text = syntax.text().slice(range.start()..);
        let pos = match text.find_char('\n') {
            None => return TextEdit::builder().finish(),
            Some(pos) => pos,
        };
        TextRange::at(range.start() + pos, TextSize::of('\n'))
    } else {
        range
    };

    let mut edit = TextEdit::builder();
    match file.syntax().covering_element(range) {
        NodeOrToken::Node(node) => {
            for token in node.descendants_with_tokens().filter_map(|it| it.into_token()) {
                remove_newlines(&mut edit, &token, range)
            }
        }
        NodeOrToken::Token(token) => remove_newlines(&mut edit, &token, range),
    };
    edit.finish()
}

fn remove_newlines(edit: &mut TextEditBuilder, token: &SyntaxToken, range: TextRange) {
    let intersection = match range.intersect(token.text_range()) {
        Some(range) => range,
        None => return,
    };

    let range = intersection - token.text_range().start();
    let text = token.text();
    for (pos, _) in text[range].bytes().enumerate().filter(|&(_, b)| b == b'\n') {
        let pos: TextSize = (pos as u32).into();
        let offset = token.text_range().start() + range.start() + pos;
        if !edit.invalidates_offset(offset) {
            remove_newline(edit, token, offset);
        }
    }
}

fn remove_newline(edit: &mut TextEditBuilder, token: &SyntaxToken, offset: TextSize) {
    if token.kind() != WHITESPACE || token.text().bytes().filter(|&b| b == b'\n').count() != 1 {
        let n_spaces_after_line_break = {
            let suff = &token.text()[TextRange::new(
                offset - token.text_range().start() + TextSize::of('\n'),
                TextSize::of(token.text()),
            )];
            suff.bytes().take_while(|&b| b == b' ').count()
        };

        let mut no_space = false;
        if let Some(string) = ast::String::cast(token.clone()) {
            if let Some(range) = string.open_quote_text_range() {
                cov_mark::hit!(join_string_literal_open_quote);
                no_space |= range.end() == offset;
            }
            if let Some(range) = string.close_quote_text_range() {
                cov_mark::hit!(join_string_literal_close_quote);
                no_space |= range.start()
                    == offset
                        + TextSize::of('\n')
                        + TextSize::try_from(n_spaces_after_line_break).unwrap();
            }
        }

        let range = TextRange::at(offset, ((n_spaces_after_line_break + 1) as u32).into());
        let replace_with = if no_space { "" } else { " " };
        edit.replace(range, replace_with.to_string());
        return;
    }

    // The node is between two other nodes
    let (prev, next) = match (token.prev_sibling_or_token(), token.next_sibling_or_token()) {
        (Some(prev), Some(next)) => (prev, next),
        _ => return,
    };

    if is_trailing_comma(prev.kind(), next.kind()) {
        // Removes: trailing comma, newline (incl. surrounding whitespace)
        edit.delete(TextRange::new(prev.text_range().start(), token.text_range().end()));
        return;
    }

    if prev.kind() == T![,] && next.kind() == T!['}'] {
        // Removes: comma, newline (incl. surrounding whitespace)
        let space = if let Some(left) = prev.prev_sibling_or_token() {
            compute_ws(left.kind(), next.kind())
        } else {
            " "
        };
        edit.replace(
            TextRange::new(prev.text_range().start(), token.text_range().end()),
            space.to_string(),
        );
        return;
    }

    if let (Some(_), Some(next)) = (
        prev.as_token().cloned().and_then(ast::Comment::cast),
        next.as_token().cloned().and_then(ast::Comment::cast),
    ) {
        // Removes: newline (incl. surrounding whitespace), start of the next comment
        edit.delete(TextRange::new(
            token.text_range().start(),
            next.syntax().text_range().start() + TextSize::of(next.prefix()),
        ));
        return;
    }

    if let (Some(prev), Some(_next)) = (as_if_expr(&prev), as_if_expr(&next)) {
        match prev.else_token() {
            Some(_) => cov_mark::hit!(join_two_ifs_with_existing_else),
            None => {
                cov_mark::hit!(join_two_ifs);
                edit.replace(token.text_range(), " else ".to_string());
                return;
            }
        }
    }

    // Special case that turns something like:
    //
    // ```
    // my_function({$0
    //    <some-expr>
    // })
    // ```
    //
    // into `my_function(<some-expr>)`
    if join_single_expr_block(edit, token).is_some() {
        return;
    }
    // ditto for
    //
    // ```
    // use foo::{$0
    //    bar
    // };
    // ```
    if join_single_use_tree(edit, token).is_some() {
        return;
    }

    // Remove newline but add a computed amount of whitespace characters
    edit.replace(token.text_range(), compute_ws(prev.kind(), next.kind()).to_string());
}

fn has_comma_after(node: &SyntaxNode) -> bool {
    match non_trivia_sibling(node.clone().into(), Direction::Next) {
        Some(n) => n.kind() == T![,],
        _ => false,
    }
}

fn join_single_expr_block(edit: &mut TextEditBuilder, token: &SyntaxToken) -> Option<()> {
    let block_expr = ast::BlockExpr::cast(token.parent()?)?;
    if !block_expr.is_standalone() {
        return None;
    }
    let expr = extract_trivial_expression(&block_expr)?;

    let block_range = block_expr.syntax().text_range();
    let mut buf = expr.syntax().text().to_string();

    // Match block needs to have a comma after the block
    if let Some(match_arm) = block_expr.syntax().parent().and_then(ast::MatchArm::cast) {
        if !has_comma_after(match_arm.syntax()) {
            buf.push(',');
        }
    }

    edit.replace(block_range, buf);

    Some(())
}

fn join_single_use_tree(edit: &mut TextEditBuilder, token: &SyntaxToken) -> Option<()> {
    let use_tree_list = ast::UseTreeList::cast(token.parent()?)?;
    let (tree,) = use_tree_list.use_trees().collect_tuple()?;
    edit.replace(use_tree_list.syntax().text_range(), tree.syntax().text().to_string());
    Some(())
}

fn is_trailing_comma(left: SyntaxKind, right: SyntaxKind) -> bool {
    matches!((left, right), (T![,], T![')'] | T![']']))
}

fn as_if_expr(element: &SyntaxElement) -> Option<ast::IfExpr> {
    let mut node = element.as_node()?.clone();
    if let Some(stmt) = ast::ExprStmt::cast(node.clone()) {
        node = stmt.expr()?.syntax().clone();
    }
    ast::IfExpr::cast(node)
}

fn compute_ws(left: SyntaxKind, right: SyntaxKind) -> &'static str {
    match left {
        T!['('] | T!['['] => return "",
        T!['{'] => {
            if let USE_TREE = right {
                return "";
            }
        }
        _ => (),
    }
    match right {
        T![')'] | T![']'] => return "",
        T!['}'] => {
            if let USE_TREE = left {
                return "";
            }
        }
        T![.] => return "",
        _ => (),
    }
    " "
}

#[cfg(test)]
mod tests {
    use syntax::SourceFile;
    use test_utils::{add_cursor, assert_eq_text, extract_offset, extract_range};

    use super::*;

    fn check_join_lines(ra_fixture_before: &str, ra_fixture_after: &str) {
        let (before_cursor_pos, before) = extract_offset(ra_fixture_before);
        let file = SourceFile::parse(&before).ok().unwrap();

        let range = TextRange::empty(before_cursor_pos);
        let result = join_lines(&file, range);

        let actual = {
            let mut actual = before;
            result.apply(&mut actual);
            actual
        };
        let actual_cursor_pos = result
            .apply_to_offset(before_cursor_pos)
            .expect("cursor position is affected by the edit");
        let actual = add_cursor(&actual, actual_cursor_pos);
        assert_eq_text!(ra_fixture_after, &actual);
    }

    #[test]
    fn test_join_lines_comma() {
        check_join_lines(
            r"
fn foo() {
    $0foo(1,
    )
}
",
            r"
fn foo() {
    $0foo(1)
}
",
        );
    }

    #[test]
    fn test_join_lines_lambda_block() {
        check_join_lines(
            r"
pub fn reparse(&self, edit: &AtomTextEdit) -> File {
    $0self.incremental_reparse(edit).unwrap_or_else(|| {
        self.full_reparse(edit)
    })
}
",
            r"
pub fn reparse(&self, edit: &AtomTextEdit) -> File {
    $0self.incremental_reparse(edit).unwrap_or_else(|| self.full_reparse(edit))
}
",
        );
    }

    #[test]
    fn test_join_lines_block() {
        check_join_lines(
            r"
fn foo() {
    foo($0{
        92
    })
}",
            r"
fn foo() {
    foo($092)
}",
        );
    }

    #[test]
    fn test_join_lines_diverging_block() {
        check_join_lines(
            r"
fn foo() {
    loop {
        match x {
            92 => $0{
                continue;
            }
        }
    }
}
        ",
            r"
fn foo() {
    loop {
        match x {
            92 => $0continue,
        }
    }
}
        ",
        );
    }

    #[test]
    fn join_lines_adds_comma_for_block_in_match_arm() {
        check_join_lines(
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0{
            u.foo()
        }
        Err(v) => v,
    }
}",
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0u.foo(),
        Err(v) => v,
    }
}",
        );
    }

    #[test]
    fn join_lines_multiline_in_block() {
        check_join_lines(
            r"
fn foo() {
    match ty {
        $0 Some(ty) => {
            match ty {
                _ => false,
            }
        }
        _ => true,
    }
}
",
            r"
fn foo() {
    match ty {
        $0 Some(ty) => match ty {
                _ => false,
            },
        _ => true,
    }
}
",
        );
    }

    #[test]
    fn join_lines_keeps_comma_for_block_in_match_arm() {
        // We already have a comma
        check_join_lines(
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0{
            u.foo()
        },
        Err(v) => v,
    }
}",
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0u.foo(),
        Err(v) => v,
    }
}",
        );

        // comma with whitespace between brace and ,
        check_join_lines(
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0{
            u.foo()
        }    ,
        Err(v) => v,
    }
}",
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0u.foo()    ,
        Err(v) => v,
    }
}",
        );

        // comma with newline between brace and ,
        check_join_lines(
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0{
            u.foo()
        }
        ,
        Err(v) => v,
    }
}",
            r"
fn foo(e: Result<U, V>) {
    match e {
        Ok(u) => $0u.foo()
        ,
        Err(v) => v,
    }
}",
        );
    }

    #[test]
    fn join_lines_keeps_comma_with_single_arg_tuple() {
        // A single arg tuple
        check_join_lines(
            r"
fn foo() {
    let x = ($0{
       4
    },);
}",
            r"
fn foo() {
    let x = ($04,);
}",
        );

        // single arg tuple with whitespace between brace and comma
        check_join_lines(
            r"
fn foo() {
    let x = ($0{
       4
    }   ,);
}",
            r"
fn foo() {
    let x = ($04   ,);
}",
        );

        // single arg tuple with newline between brace and comma
        check_join_lines(
            r"
fn foo() {
    let x = ($0{
       4
    }
    ,);
}",
            r"
fn foo() {
    let x = ($04
    ,);
}",
        );
    }

    #[test]
    fn test_join_lines_use_items_left() {
        // No space after the '{'
        check_join_lines(
            r"
$0use syntax::{
    TextSize, TextRange,
};",
            r"
$0use syntax::{TextSize, TextRange,
};",
        );
    }

    #[test]
    fn test_join_lines_use_items_right() {
        // No space after the '}'
        check_join_lines(
            r"
use syntax::{
$0    TextSize, TextRange
};",
            r"
use syntax::{
$0    TextSize, TextRange};",
        );
    }

    #[test]
    fn test_join_lines_use_items_right_comma() {
        // No space after the '}'
        check_join_lines(
            r"
use syntax::{
$0    TextSize, TextRange,
};",
            r"
use syntax::{
$0    TextSize, TextRange};",
        );
    }

    #[test]
    fn test_join_lines_use_tree() {
        check_join_lines(
            r"
use syntax::{
    algo::$0{
        find_token_at_offset,
    },
    ast,
};",
            r"
use syntax::{
    algo::$0find_token_at_offset,
    ast,
};",
        );
    }

    #[test]
    fn test_join_lines_normal_comments() {
        check_join_lines(
            r"
fn foo() {
    // Hello$0
    // world!
}
",
            r"
fn foo() {
    // Hello$0 world!
}
",
        );
    }

    #[test]
    fn test_join_lines_doc_comments() {
        check_join_lines(
            r"
fn foo() {
    /// Hello$0
    /// world!
}
",
            r"
fn foo() {
    /// Hello$0 world!
}
",
        );
    }

    #[test]
    fn test_join_lines_mod_comments() {
        check_join_lines(
            r"
fn foo() {
    //! Hello$0
    //! world!
}
",
            r"
fn foo() {
    //! Hello$0 world!
}
",
        );
    }

    #[test]
    fn test_join_lines_multiline_comments_1() {
        check_join_lines(
            r"
fn foo() {
    // Hello$0
    /* world! */
}
",
            r"
fn foo() {
    // Hello$0 world! */
}
",
        );
    }

    #[test]
    fn test_join_lines_multiline_comments_2() {
        check_join_lines(
            r"
fn foo() {
    // The$0
    /* quick
    brown
    fox! */
}
",
            r"
fn foo() {
    // The$0 quick
    brown
    fox! */
}
",
        );
    }

    fn check_join_lines_sel(ra_fixture_before: &str, ra_fixture_after: &str) {
        let (sel, before) = extract_range(ra_fixture_before);
        let parse = SourceFile::parse(&before);
        let result = join_lines(&parse.tree(), sel);
        let actual = {
            let mut actual = before;
            result.apply(&mut actual);
            actual
        };
        assert_eq_text!(ra_fixture_after, &actual);
    }

    #[test]
    fn test_join_lines_selection_fn_args() {
        check_join_lines_sel(
            r"
fn foo() {
    $0foo(1,
        2,
        3,
    $0)
}
    ",
            r"
fn foo() {
    foo(1, 2, 3)
}
    ",
        );
    }

    #[test]
    fn test_join_lines_selection_struct() {
        check_join_lines_sel(
            r"
struct Foo $0{
    f: u32,
}$0
    ",
            r"
struct Foo { f: u32 }
    ",
        );
    }

    #[test]
    fn test_join_lines_selection_dot_chain() {
        check_join_lines_sel(
            r"
fn foo() {
    join($0type_params.type_params()
            .filter_map(|it| it.name())
            .map(|it| it.text())$0)
}",
            r"
fn foo() {
    join(type_params.type_params().filter_map(|it| it.name()).map(|it| it.text()))
}",
        );
    }

    #[test]
    fn test_join_lines_selection_lambda_block_body() {
        check_join_lines_sel(
            r"
pub fn handle_find_matching_brace() {
    params.offsets
        .map(|offset| $0{
            world.analysis().matching_brace(&file, offset).unwrap_or(offset)
        }$0)
        .collect();
}",
            r"
pub fn handle_find_matching_brace() {
    params.offsets
        .map(|offset| world.analysis().matching_brace(&file, offset).unwrap_or(offset))
        .collect();
}",
        );
    }

    #[test]
    fn test_join_lines_commented_block() {
        check_join_lines(
            r"
fn main() {
    let _ = {
        // $0foo
        // bar
        92
    };
}
        ",
            r"
fn main() {
    let _ = {
        // $0foo bar
        92
    };
}
        ",
        )
    }

    #[test]
    fn join_lines_mandatory_blocks_block() {
        check_join_lines(
            r"
$0fn foo() {
    92
}
        ",
            r"
$0fn foo() { 92
}
        ",
        );

        check_join_lines(
            r"
fn foo() {
    $0if true {
        92
    }
}
        ",
            r"
fn foo() {
    $0if true { 92
    }
}
        ",
        );

        check_join_lines(
            r"
fn foo() {
    $0loop {
        92
    }
}
        ",
            r"
fn foo() {
    $0loop { 92
    }
}
        ",
        );

        check_join_lines(
            r"
fn foo() {
    $0unsafe {
        92
    }
}
        ",
            r"
fn foo() {
    $0unsafe { 92
    }
}
        ",
        );
    }

    #[test]
    fn join_string_literal() {
        {
            cov_mark::check!(join_string_literal_open_quote);
            check_join_lines(
                r#"
fn main() {
    $0"
hello
";
}
"#,
                r#"
fn main() {
    $0"hello
";
}
"#,
            );
        }

        {
            cov_mark::check!(join_string_literal_close_quote);
            check_join_lines(
                r#"
fn main() {
    $0"hello
";
}
"#,
                r#"
fn main() {
    $0"hello";
}
"#,
            );
            check_join_lines(
                r#"
fn main() {
    $0r"hello
    ";
}
"#,
                r#"
fn main() {
    $0r"hello";
}
"#,
            );
        }

        check_join_lines(
            r#"
fn main() {
    "
$0hello
world
";
}
"#,
            r#"
fn main() {
    "
$0hello world
";
}
"#,
        );
    }

    #[test]
    fn join_last_line_empty() {
        check_join_lines(
            r#"
fn main() {$0}
"#,
            r#"
fn main() {$0}
"#,
        );
    }

    #[test]
    fn join_two_ifs() {
        cov_mark::check!(join_two_ifs);
        check_join_lines(
            r#"
fn main() {
    if foo {

    }$0
    if bar {

    }
}
"#,
            r#"
fn main() {
    if foo {

    }$0 else if bar {

    }
}
"#,
        );
    }

    #[test]
    fn join_two_ifs_with_existing_else() {
        cov_mark::check!(join_two_ifs_with_existing_else);
        check_join_lines(
            r#"
fn main() {
    if foo {

    } else {

    }$0
    if bar {

    }
}
"#,
            r#"
fn main() {
    if foo {

    } else {

    }$0 if bar {

    }
}
"#,
        );
    }
}
