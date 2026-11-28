use std::fs;

use archlens::{AnalysisConfig, analyze};

#[test]
fn python_imports_resolve_relative_submodules_and_other_package_roots() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let entry = "app/src/client/consumer/entry.py";
    let initializer = "app/src/client/consumer/__init__.py";
    let incomplete = "app/src/client/incomplete/__init__.py";
    let adapter = "provider/src/exported/adapter.py";
    for (path, contents) in [
        ("app/pyproject.toml", "[project]\nname = 'consumer-app'\n"),
        (
            "provider/pyproject.toml",
            "[tool.poetry]\nname = 'export-provider'\n",
        ),
        (
            entry,
            concat!(
                "from . import sibling, hook\n",
                "from .tools import *\n",
                "from ..shared import codec\n",
                "from exported import adapter\n",
                "from exported import FEATURE\n",
            ),
        ),
        ("app/src/client/__init__.py", ""),
        (initializer, "hook = True\n"),
        (incomplete, "from . import missing\n"),
        ("app/src/client/consumer/sibling.py", ""),
        ("app/src/client/consumer/tools.py", ""),
        ("app/src/client/shared/__init__.py", ""),
        ("app/src/client/shared/codec.py", ""),
        ("provider/src/exported/__init__.py", "FEATURE = True\n"),
        (adapter, ""),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture");
        fs::write(path, contents).expect("write fixture");
    }

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let graph = analyze(&config).expect("analyze Python imports").file_graph;
    let dependencies: Vec<_> = graph
        .edges
        .iter()
        .map(|edge| {
            assert_eq!(edge.from, entry);
            (edge.to.as_str(), edge.line)
        })
        .collect();
    assert_eq!(
        dependencies,
        [
            (initializer, 1),
            ("app/src/client/consumer/sibling.py", 1),
            ("app/src/client/consumer/tools.py", 2),
            ("app/src/client/shared/codec.py", 3),
            ("provider/src/exported/__init__.py", 5),
            (adapter, 4),
        ],
    );

    let file = |path| {
        graph
            .files
            .iter()
            .find(|file| file.path == path)
            .expect("resolved file")
    };
    assert_eq!(file(entry).package.as_deref(), Some("consumer-app"));
    assert_eq!(file(adapter).package.as_deref(), Some("export-provider"));
    assert_eq!(file(incomplete).unresolved_imports, [".::missing"]);
}
