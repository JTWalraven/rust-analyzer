use super::*;

use hir::PrefixKind;
use test_utils::{assert_eq_text, extract_range_or_offset, CURSOR_MARKER};

#[test]
fn respects_cfg_attr_fn() {
    check(
        r"bar::Bar",
        r#"
#[cfg(test)]
fn foo() {$0}
"#,
        r#"
#[cfg(test)]
fn foo() {
use bar::Bar;
}
"#,
        ImportGranularity::Crate,
    );
}

#[test]
fn respects_cfg_attr_const() {
    check(
        r"bar::Bar",
        r#"
#[cfg(test)]
const FOO: Bar = {$0};
"#,
        r#"
#[cfg(test)]
const FOO: Bar = {
use bar::Bar;
};
"#,
        ImportGranularity::Crate,
    );
}

#[test]
fn insert_skips_lone_glob_imports() {
    check(
        "use foo::baz::A",
        r"
use foo::bar::*;
",
        r"
use foo::bar::*;
use foo::baz::A;
",
        ImportGranularity::Crate,
    );
}

#[test]
fn insert_not_group() {
    cov_mark::check!(insert_no_grouping_last);
    check_with_config(
        "use external_crate2::bar::A",
        r"
use std::bar::B;
use external_crate::bar::A;
use crate::bar::A;
use self::bar::A;
use super::bar::A;",
        r"
use std::bar::B;
use external_crate::bar::A;
use crate::bar::A;
use self::bar::A;
use super::bar::A;
use external_crate2::bar::A;",
        &InsertUseConfig {
            granularity: ImportGranularity::Item,
            enforce_granularity: true,
            prefix_kind: PrefixKind::Plain,
            group: false,
            skip_glob_imports: true,
        },
    );
}

#[test]
fn insert_not_group_empty() {
    cov_mark::check!(insert_no_grouping_last2);
    check_with_config(
        "use external_crate2::bar::A",
        r"",
        r"use external_crate2::bar::A;

",
        &InsertUseConfig {
            granularity: ImportGranularity::Item,
            enforce_granularity: true,
            prefix_kind: PrefixKind::Plain,
            group: false,
            skip_glob_imports: true,
        },
    );
}

#[test]
fn insert_existing() {
    check_crate("std::fs", "use std::fs;", "use std::fs;")
}

#[test]
fn insert_start() {
    check_none(
        "std::bar::AA",
        r"
use std::bar::B;
use std::bar::D;
use std::bar::F;
use std::bar::G;",
        r"
use std::bar::AA;
use std::bar::B;
use std::bar::D;
use std::bar::F;
use std::bar::G;",
    )
}

#[test]
fn insert_start_indent() {
    check_none(
        "std::bar::AA",
        r"
    use std::bar::B;
    use std::bar::C;",
        r"
    use std::bar::AA;
    use std::bar::B;
    use std::bar::C;",
    );
}

#[test]
fn insert_middle() {
    cov_mark::check!(insert_group);
    check_none(
        "std::bar::EE",
        r"
use std::bar::A;
use std::bar::D;
use std::bar::F;
use std::bar::G;",
        r"
use std::bar::A;
use std::bar::D;
use std::bar::EE;
use std::bar::F;
use std::bar::G;",
    )
}

#[test]
fn insert_middle_indent() {
    check_none(
        "std::bar::EE",
        r"
    use std::bar::A;
    use std::bar::D;
    use std::bar::F;
    use std::bar::G;",
        r"
    use std::bar::A;
    use std::bar::D;
    use std::bar::EE;
    use std::bar::F;
    use std::bar::G;",
    )
}

#[test]
fn insert_end() {
    cov_mark::check!(insert_group_last);
    check_none(
        "std::bar::ZZ",
        r"
use std::bar::A;
use std::bar::D;
use std::bar::F;
use std::bar::G;",
        r"
use std::bar::A;
use std::bar::D;
use std::bar::F;
use std::bar::G;
use std::bar::ZZ;",
    )
}

#[test]
fn insert_end_indent() {
    check_none(
        "std::bar::ZZ",
        r"
    use std::bar::A;
    use std::bar::D;
    use std::bar::F;
    use std::bar::G;",
        r"
    use std::bar::A;
    use std::bar::D;
    use std::bar::F;
    use std::bar::G;
    use std::bar::ZZ;",
    )
}

#[test]
fn insert_middle_nested() {
    check_none(
        "std::bar::EE",
        r"
use std::bar::A;
use std::bar::{D, Z}; // example of weird imports due to user
use std::bar::F;
use std::bar::G;",
        r"
use std::bar::A;
use std::bar::EE;
use std::bar::{D, Z}; // example of weird imports due to user
use std::bar::F;
use std::bar::G;",
    )
}

#[test]
fn insert_middle_groups() {
    check_none(
        "foo::bar::GG",
        r"
    use std::bar::A;
    use std::bar::D;

    use foo::bar::F;
    use foo::bar::H;",
        r"
    use std::bar::A;
    use std::bar::D;

    use foo::bar::F;
    use foo::bar::GG;
    use foo::bar::H;",
    )
}

#[test]
fn insert_first_matching_group() {
    check_none(
        "foo::bar::GG",
        r"
    use foo::bar::A;
    use foo::bar::D;

    use std;

    use foo::bar::F;
    use foo::bar::H;",
        r"
    use foo::bar::A;
    use foo::bar::D;
    use foo::bar::GG;

    use std;

    use foo::bar::F;
    use foo::bar::H;",
    )
}

#[test]
fn insert_missing_group_std() {
    cov_mark::check!(insert_group_new_group);
    check_none(
        "std::fmt",
        r"
    use foo::bar::A;
    use foo::bar::D;",
        r"
    use std::fmt;

    use foo::bar::A;
    use foo::bar::D;",
    )
}

#[test]
fn insert_missing_group_self() {
    cov_mark::check!(insert_group_no_group);
    check_none(
        "self::fmt",
        r"
use foo::bar::A;
use foo::bar::D;",
        r"
use foo::bar::A;
use foo::bar::D;

use self::fmt;",
    )
}

#[test]
fn insert_no_imports() {
    check_crate(
        "foo::bar",
        "fn main() {}",
        r"use foo::bar;

fn main() {}",
    )
}

#[test]
fn insert_empty_file() {
    cov_mark::check!(insert_group_empty_file);
    // empty files will get two trailing newlines
    // this is due to the test case insert_no_imports above
    check_crate(
        "foo::bar",
        "",
        r"use foo::bar;

",
    )
}

#[test]
fn insert_empty_module() {
    cov_mark::check!(insert_group_empty_module);
    check(
        "foo::bar",
        r"
mod x {$0}
",
        r"
mod x {
    use foo::bar;
}
",
        ImportGranularity::Item,
    )
}

#[test]
fn insert_after_inner_attr() {
    cov_mark::check!(insert_group_empty_inner_attr);
    check_crate(
        "foo::bar",
        r"#![allow(unused_imports)]",
        r"#![allow(unused_imports)]

use foo::bar;",
    )
}

#[test]
fn insert_after_inner_attr2() {
    check_crate(
        "foo::bar",
        r"#![allow(unused_imports)]

#![no_std]
fn main() {}",
        r"#![allow(unused_imports)]

#![no_std]

use foo::bar;
fn main() {}",
    );
}

#[test]
fn inserts_after_single_line_inner_comments() {
    check_none(
        "foo::bar::Baz",
        "//! Single line inner comments do not allow any code before them.",
        r#"//! Single line inner comments do not allow any code before them.

use foo::bar::Baz;"#,
    );
}

#[test]
fn inserts_after_multiline_inner_comments() {
    check_none(
        "foo::bar::Baz",
        r#"/*! Multiline inner comments do not allow any code before them. */

/*! Still an inner comment, cannot place any code before. */
fn main() {}"#,
        r#"/*! Multiline inner comments do not allow any code before them. */

/*! Still an inner comment, cannot place any code before. */

use foo::bar::Baz;
fn main() {}"#,
    )
}

#[test]
fn inserts_after_all_inner_items() {
    check_none(
        "foo::bar::Baz",
        r#"#![allow(unused_imports)]
/*! Multiline line comment 2 */


//! Single line comment 1
#![no_std]
//! Single line comment 2
fn main() {}"#,
        r#"#![allow(unused_imports)]
/*! Multiline line comment 2 */


//! Single line comment 1
#![no_std]
//! Single line comment 2

use foo::bar::Baz;
fn main() {}"#,
    )
}

#[test]
fn merge_groups() {
    check_module("std::io", r"use std::fmt;", r"use std::{fmt, io};")
}

#[test]
fn merge_groups_last() {
    check_module(
        "std::io",
        r"use std::fmt::{Result, Display};",
        r"use std::fmt::{Result, Display};
use std::io;",
    )
}

#[test]
fn merge_last_into_self() {
    check_module("foo::bar::baz", r"use foo::bar;", r"use foo::bar::{self, baz};");
}

#[test]
fn merge_groups_full() {
    check_crate(
        "std::io",
        r"use std::fmt::{Result, Display};",
        r"use std::{fmt::{Result, Display}, io};",
    )
}

#[test]
fn merge_groups_long_full() {
    check_crate("std::foo::bar::Baz", r"use std::foo::bar::Qux;", r"use std::foo::bar::{Baz, Qux};")
}

#[test]
fn merge_groups_long_last() {
    check_module(
        "std::foo::bar::Baz",
        r"use std::foo::bar::Qux;",
        r"use std::foo::bar::{Baz, Qux};",
    )
}

#[test]
fn merge_groups_long_full_list() {
    check_crate(
        "std::foo::bar::Baz",
        r"use std::foo::bar::{Qux, Quux};",
        r"use std::foo::bar::{Baz, Quux, Qux};",
    )
}

#[test]
fn merge_groups_long_last_list() {
    check_module(
        "std::foo::bar::Baz",
        r"use std::foo::bar::{Qux, Quux};",
        r"use std::foo::bar::{Baz, Quux, Qux};",
    )
}

#[test]
fn merge_groups_long_full_nested() {
    check_crate(
        "std::foo::bar::Baz",
        r"use std::foo::bar::{Qux, quux::{Fez, Fizz}};",
        r"use std::foo::bar::{Baz, Qux, quux::{Fez, Fizz}};",
    )
}

#[test]
fn merge_groups_long_last_nested() {
    check_module(
        "std::foo::bar::Baz",
        r"use std::foo::bar::{Qux, quux::{Fez, Fizz}};",
        r"use std::foo::bar::Baz;
use std::foo::bar::{Qux, quux::{Fez, Fizz}};",
    )
}

#[test]
fn merge_groups_full_nested_deep() {
    check_crate(
        "std::foo::bar::quux::Baz",
        r"use std::foo::bar::{Qux, quux::{Fez, Fizz}};",
        r"use std::foo::bar::{Qux, quux::{Baz, Fez, Fizz}};",
    )
}

#[test]
fn merge_groups_full_nested_long() {
    check_crate(
        "std::foo::bar::Baz",
        r"use std::{foo::bar::Qux};",
        r"use std::{foo::bar::{Baz, Qux}};",
    );
}

#[test]
fn merge_groups_last_nested_long() {
    check_crate(
        "std::foo::bar::Baz",
        r"use std::{foo::bar::Qux};",
        r"use std::{foo::bar::{Baz, Qux}};",
    );
}

#[test]
fn merge_groups_skip_pub() {
    check_crate(
        "std::io",
        r"pub use std::fmt::{Result, Display};",
        r"pub use std::fmt::{Result, Display};
use std::io;",
    )
}

#[test]
fn merge_groups_skip_pub_crate() {
    check_crate(
        "std::io",
        r"pub(crate) use std::fmt::{Result, Display};",
        r"pub(crate) use std::fmt::{Result, Display};
use std::io;",
    )
}

#[test]
fn merge_groups_skip_attributed() {
    check_crate(
        "std::io",
        r#"
#[cfg(feature = "gated")] use std::fmt::{Result, Display};
"#,
        r#"
#[cfg(feature = "gated")] use std::fmt::{Result, Display};
use std::io;
"#,
    )
}

#[test]
fn split_out_merge() {
    // FIXME: This is suboptimal, we want to get `use std::fmt::{self, Result}`
    // instead.
    check_module(
        "std::fmt::Result",
        r"use std::{fmt, io};",
        r"use std::fmt::Result;
use std::{fmt, io};",
    )
}

#[test]
fn merge_into_module_import() {
    check_crate("std::fmt::Result", r"use std::{fmt, io};", r"use std::{fmt::{self, Result}, io};")
}

#[test]
fn merge_groups_self() {
    check_crate("std::fmt::Debug", r"use std::fmt;", r"use std::fmt::{self, Debug};")
}

#[test]
fn merge_mod_into_glob() {
    check_with_config(
        "token::TokenKind",
        r"use token::TokenKind::*;",
        r"use token::TokenKind::{*, self};",
        &InsertUseConfig {
            granularity: ImportGranularity::Crate,
            enforce_granularity: true,
            prefix_kind: PrefixKind::Plain,
            group: false,
            skip_glob_imports: false,
        },
    )
    // FIXME: have it emit `use token::TokenKind::{self, *}`?
}

#[test]
fn merge_self_glob() {
    check_with_config(
        "self",
        r"use self::*;",
        r"use self::{*, self};",
        &InsertUseConfig {
            granularity: ImportGranularity::Crate,
            enforce_granularity: true,
            prefix_kind: PrefixKind::Plain,
            group: false,
            skip_glob_imports: false,
        },
    )
    // FIXME: have it emit `use {self, *}`?
}

#[test]
fn merge_glob_nested() {
    check_crate(
        "foo::bar::quux::Fez",
        r"use foo::bar::{Baz, quux::*};",
        r"use foo::bar::{Baz, quux::{self::*, Fez}};",
    )
}

#[test]
fn merge_nested_considers_first_segments() {
    check_crate(
        "hir_ty::display::write_bounds_like_dyn_trait",
        r"use hir_ty::{autoderef, display::{HirDisplayError, HirFormatter}, method_resolution};",
        r"use hir_ty::{autoderef, display::{HirDisplayError, HirFormatter, write_bounds_like_dyn_trait}, method_resolution};",
    );
}

#[test]
fn skip_merge_last_too_long() {
    check_module(
        "foo::bar",
        r"use foo::bar::baz::Qux;",
        r"use foo::bar;
use foo::bar::baz::Qux;",
    );
}

#[test]
fn skip_merge_last_too_long2() {
    check_module(
        "foo::bar::baz::Qux",
        r"use foo::bar;",
        r"use foo::bar;
use foo::bar::baz::Qux;",
    );
}

#[test]
fn insert_short_before_long() {
    check_none(
        "foo::bar",
        r"use foo::bar::baz::Qux;",
        r"use foo::bar;
use foo::bar::baz::Qux;",
    );
}

#[test]
fn merge_last_fail() {
    check_merge_only_fail(
        r"use foo::bar::{baz::{Qux, Fez}};",
        r"use foo::bar::{baaz::{Quux, Feez}};",
        MergeBehavior::Module,
    );
}

#[test]
fn merge_last_fail1() {
    check_merge_only_fail(
        r"use foo::bar::{baz::{Qux, Fez}};",
        r"use foo::bar::baaz::{Quux, Feez};",
        MergeBehavior::Module,
    );
}

#[test]
fn merge_last_fail2() {
    check_merge_only_fail(
        r"use foo::bar::baz::{Qux, Fez};",
        r"use foo::bar::{baaz::{Quux, Feez}};",
        MergeBehavior::Module,
    );
}

#[test]
fn merge_last_fail3() {
    check_merge_only_fail(
        r"use foo::bar::baz::{Qux, Fez};",
        r"use foo::bar::baaz::{Quux, Feez};",
        MergeBehavior::Module,
    );
}

#[test]
fn guess_empty() {
    check_guess("", ImportGranularityGuess::Unknown);
}

#[test]
fn guess_single() {
    check_guess(r"use foo::{baz::{qux, quux}, bar};", ImportGranularityGuess::Crate);
    check_guess(r"use foo::bar;", ImportGranularityGuess::Unknown);
    check_guess(r"use foo::bar::{baz, qux};", ImportGranularityGuess::CrateOrModule);
}

#[test]
fn guess_unknown() {
    check_guess(
        r"
use foo::bar::baz;
use oof::rab::xuq;
",
        ImportGranularityGuess::Unknown,
    );
}

#[test]
fn guess_item() {
    check_guess(
        r"
use foo::bar::baz;
use foo::bar::qux;
",
        ImportGranularityGuess::Item,
    );
}

#[test]
fn guess_module_or_item() {
    check_guess(
        r"
use foo::bar::Bar;
use foo::qux;
",
        ImportGranularityGuess::ModuleOrItem,
    );
    check_guess(
        r"
use foo::bar::Bar;
use foo::bar;
",
        ImportGranularityGuess::ModuleOrItem,
    );
}

#[test]
fn guess_module() {
    check_guess(
        r"
use foo::bar::baz;
use foo::bar::{qux, quux};
",
        ImportGranularityGuess::Module,
    );
    // this is a rather odd case, technically this file isn't following any style properly.
    check_guess(
        r"
use foo::bar::baz;
use foo::{baz::{qux, quux}, bar};
",
        ImportGranularityGuess::Module,
    );
    check_guess(
        r"
use foo::bar::Bar;
use foo::baz::Baz;
use foo::{Foo, Qux};
",
        ImportGranularityGuess::Module,
    );
}

#[test]
fn guess_crate_or_module() {
    check_guess(
        r"
use foo::bar::baz;
use oof::bar::{qux, quux};
",
        ImportGranularityGuess::CrateOrModule,
    );
}

#[test]
fn guess_crate() {
    check_guess(
        r"
use frob::bar::baz;
use foo::{baz::{qux, quux}, bar};
",
        ImportGranularityGuess::Crate,
    );
}

#[test]
fn guess_skips_differing_vis() {
    check_guess(
        r"
use foo::bar::baz;
pub use foo::bar::qux;
",
        ImportGranularityGuess::Unknown,
    );
}

#[test]
fn guess_skips_differing_attrs() {
    check_guess(
        r"
pub use foo::bar::baz;
#[doc(hidden)]
pub use foo::bar::qux;
",
        ImportGranularityGuess::Unknown,
    );
}

#[test]
fn guess_grouping_matters() {
    check_guess(
        r"
use foo::bar::baz;
use oof::bar::baz;
use foo::bar::qux;
",
        ImportGranularityGuess::Unknown,
    );
}

fn check_with_config(
    path: &str,
    ra_fixture_before: &str,
    ra_fixture_after: &str,
    config: &InsertUseConfig,
) {
    let (text, pos) = if ra_fixture_before.contains(CURSOR_MARKER) {
        let (range_or_offset, text) = extract_range_or_offset(ra_fixture_before);
        (text, Some(range_or_offset))
    } else {
        (ra_fixture_before.to_owned(), None)
    };
    let syntax = ast::SourceFile::parse(&text).tree().syntax().clone_for_update();
    let file = pos
        .and_then(|pos| syntax.token_at_offset(pos.expect_offset()).next()?.parent())
        .and_then(|it| super::ImportScope::find_insert_use_container(&it))
        .or_else(|| super::ImportScope::from(syntax))
        .unwrap();
    let path = ast::SourceFile::parse(&format!("use {};", path))
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Path::cast)
        .unwrap();

    insert_use(&file, path, config);
    let result = file.as_syntax_node().ancestors().last().unwrap().to_string();
    assert_eq_text!(ra_fixture_after, &result);
}

fn check(
    path: &str,
    ra_fixture_before: &str,
    ra_fixture_after: &str,
    granularity: ImportGranularity,
) {
    check_with_config(
        path,
        ra_fixture_before,
        ra_fixture_after,
        &InsertUseConfig {
            granularity,
            enforce_granularity: true,
            prefix_kind: PrefixKind::Plain,
            group: true,
            skip_glob_imports: true,
        },
    )
}

fn check_crate(path: &str, ra_fixture_before: &str, ra_fixture_after: &str) {
    check(path, ra_fixture_before, ra_fixture_after, ImportGranularity::Crate)
}

fn check_module(path: &str, ra_fixture_before: &str, ra_fixture_after: &str) {
    check(path, ra_fixture_before, ra_fixture_after, ImportGranularity::Module)
}

fn check_none(path: &str, ra_fixture_before: &str, ra_fixture_after: &str) {
    check(path, ra_fixture_before, ra_fixture_after, ImportGranularity::Item)
}

fn check_merge_only_fail(ra_fixture0: &str, ra_fixture1: &str, mb: MergeBehavior) {
    let use0 = ast::SourceFile::parse(ra_fixture0)
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Use::cast)
        .unwrap();

    let use1 = ast::SourceFile::parse(ra_fixture1)
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Use::cast)
        .unwrap();

    let result = try_merge_imports(&use0, &use1, mb);
    assert_eq!(result.map(|u| u.to_string()), None);
}

fn check_guess(ra_fixture: &str, expected: ImportGranularityGuess) {
    let syntax = ast::SourceFile::parse(ra_fixture).tree().syntax().clone();
    let file = super::ImportScope::from(syntax).unwrap();
    assert_eq!(file.guess_granularity_from_scope(), expected);
}
