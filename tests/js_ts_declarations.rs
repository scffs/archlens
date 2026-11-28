use archlens::Language;
use archlens::domain::model::{ParsedFile, SymbolKind, Visibility};
use archlens::parser::parse_source;

fn declarations(parsed: &ParsedFile) -> Vec<(&str, SymbolKind, Visibility, bool, usize)> {
    parsed
        .declarations
        .iter()
        .map(|decl| {
            (
                decl.name.as_str(),
                decl.kind,
                decl.visibility,
                decl.is_abstract,
                decl.line,
            )
        })
        .collect()
}

#[test]
fn typescript_preserves_public_contracts_and_concrete_declarations() {
    use SymbolKind::{AbstractClass, Class, Enum, Interface, TypeAlias};
    use Visibility::{Private, Public};

    let source = r"export interface Port { read(): string }
export abstract class Adapter { abstract read(): string }
export class Concrete { read() { return 'x'; } }
export enum Mode { Read }
export type Options = { retries: number };
export type Callback = (value: string) => string;
export type Wrapped = ({ limit: number });
export type Combined = Options & { trace: boolean };
export type Label = string;
export type Literal = 'fast' | 'slow';
export declare class Platform {}
class PrivateHelper {}
";
    let parsed = parse_source("contracts.ts", source, Language::TypeScript, false).unwrap();

    assert_eq!(
        declarations(&parsed),
        [
            ("Port", Interface, Public, true, 1),
            ("Adapter", AbstractClass, Public, true, 2),
            ("Concrete", Class, Public, false, 3),
            ("Mode", Enum, Public, false, 4),
            ("Options", TypeAlias, Public, true, 5),
            ("Callback", TypeAlias, Public, true, 6),
            ("Wrapped", TypeAlias, Public, true, 7),
            ("Combined", TypeAlias, Public, true, 8),
            ("Platform", Class, Public, false, 11),
            ("PrivateHelper", Class, Private, false, 12),
        ],
    );
}

#[test]
fn javascript_reports_function_and_class_initializers_on_their_own_lines() {
    use SymbolKind::{Class, Function};
    use Visibility::{Private, Public};

    let source = r"const helper = () => 0;
export const
    task = function () {},
    Factory = class {};
export var legacy = () => 1;
const literal = 1;
";
    let parsed = parse_source("jobs.js", source, Language::JavaScript, false).unwrap();

    assert_eq!(
        declarations(&parsed),
        [
            ("helper", Function, Private, false, 1),
            ("task", Function, Public, false, 3),
            ("Factory", Class, Public, false, 4),
            ("legacy", Function, Public, false, 5),
        ],
    );
}

#[test]
fn declarations_in_a_vue_script_keep_component_source_lines() {
    use SymbolKind::{AbstractClass, Component, Function};
    use Visibility::Public;

    let source = r"<template><div /></template>
<script lang='ts'>
export abstract class Service {}
export const run = () => null;
</script>
";
    let parsed = parse_source("Page.vue", source, Language::Vue, false).unwrap();

    assert_eq!(
        declarations(&parsed),
        [
            ("Service", AbstractClass, Public, true, 3),
            ("run", Function, Public, false, 4),
            ("Page", Component, Public, false, 1),
        ],
    );
}
