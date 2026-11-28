use archlens::Language;
use archlens::domain::model::SymbolKind::{Class, Function, Protocol};
use archlens::domain::model::Visibility::{Private, Public};
use archlens::parser::parse_source;

#[test]
fn python_reports_decorated_functions_and_classifies_public_symbols() {
    let source = "\
import abc
from abc import ABCMeta as ContractMeta, abstractmethod
from functools import cache

@cache
def draw():
    def nested():
        return 1
    return nested

def _helper(): pass
def __getattr__(name): raise AttributeError(name)

class Plain: pass

class Reader(metaclass=ContractMeta):
    @abstractmethod
    def read(self): ...

class Writer(metaclass=ContractMeta):
    @abc.abstractmethod
    def write(self): ...
";
    let parsed =
        parse_source("package/ports.py", source, Language::Python, false).expect("parse Python");
    let declarations: Vec<_> = parsed
        .declarations
        .iter()
        .map(|symbol| {
            (
                symbol.name.as_str(),
                symbol.kind,
                symbol.visibility,
                symbol.is_abstract,
                symbol.line,
            )
        })
        .collect();

    assert_eq!(
        declarations,
        [
            ("draw", Function, Public, false, 6),
            ("_helper", Function, Private, false, 11),
            ("__getattr__", Function, Public, false, 12),
            ("Plain", Class, Public, false, 14),
            ("Reader", Protocol, Public, true, 16),
            ("Writer", Protocol, Public, true, 20),
        ]
    );
}
