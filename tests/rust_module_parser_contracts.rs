use archlens::parser::parse_source;
use archlens::{ImportKind, Language};

#[test]
fn rust_parser_preserves_module_dependencies_across_test_attributes_and_comments() {
    let source = r"mod external;
mod feature {
    use crate::api::Feature;
}
mod test {
    use crate::testkit::Singular;
}
mod tests {
    use crate::testkit::Plural;
}
#[cfg( test )]
// A comment still belongs to the declaration below.
mod suite;
#[cfg(test)]
/* A block comment also preserves the attribute. */
mod tagged {
    use crate::testkit::Tagged;
}
#[allow(dead_code)]
mod always;
";

    for include_tests in [false, true] {
        let parsed =
            parse_source("src/lib.rs", source, Language::Rust, include_tests).expect("parse Rust");
        let imports: Vec<_> = parsed
            .raw_imports
            .iter()
            .map(|import| (import.specifier.as_str(), import.kind, import.line))
            .collect();
        let mut expected = vec![
            ("mod::external", ImportKind::ModuleDecl, 1),
            ("crate::api::Feature", ImportKind::Import, 3),
        ];
        if include_tests {
            expected.extend([
                ("crate::testkit::Singular", ImportKind::Import, 6),
                ("crate::testkit::Plural", ImportKind::Import, 9),
                ("mod::suite", ImportKind::ModuleDecl, 13),
                ("crate::testkit::Tagged", ImportKind::Import, 17),
            ]);
            assert_eq!(parsed.cfg_test_mods, [] as [String; 0]);
        } else {
            assert_eq!(parsed.cfg_test_mods, ["suite"]);
        }
        expected.push(("mod::always", ImportKind::ModuleDecl, 20));

        assert_eq!(imports, expected, "include_tests = {include_tests}");
    }
}
