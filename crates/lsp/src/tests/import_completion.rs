use super::support::{RequestFixture, check_completions_at};
use crate::vfs::VfsPath;
use crop::Rope;
use lsp_types::{DidChangeWatchedFilesParams, FileChangeType, FileEvent, Position, Url};
use snapbox::str;
use std::time::Duration;

#[tokio::test(flavor = "current_thread")]
async fn remappings_change_refreshes_import_completion_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false

        //- /remappings.txt
        pkg/=lib/old/

        //- /src/Main.sol open
        import "pkg/$1";

        //- /lib/old/Old.sol
        contract Old {}

        //- /lib/new/New.sol
        contract New {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    fixture.check_completions_in(
        &mut state,
        &["$1"],
        str![[r#"
pkg/Old.sol File filter="pkg/Old.sol" edit=0:8-0:12
| pkg/Old.sol

"#]],
    );

    std::fs::write(fixture.project_path("/remappings.txt"), "pkg/=lib/new/\n").unwrap();
    let remappings_uri = Url::from_file_path(fixture.project_path("/remappings.txt")).unwrap();
    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri: remappings_uri, typ: FileChangeType::CHANGED }],
        },
    );
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("analysis after remappings change should finish")
        .unwrap();

    fixture.check_completions_in(
        &mut state,
        &["$1"],
        str![[r#"
pkg/New.sol File filter="pkg/New.sol" edit=0:8-0:12
| pkg/New.sol

"#]],
    );
}

#[tokio::test(flavor = "current_thread")]
async fn open_overlay_changes_invalidate_import_completion_cache() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./$1";

        //- /src/Existing.sol
        contract Existing {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    fixture.check_completions_in(
        &mut state,
        &["$1"],
        str![[r#"
./Existing.sol File filter="./Existing.sol" edit=0:8-0:10
| ./Existing.sol
./Main.sol File filter="./Main.sol" edit=0:8-0:10
| ./Main.sol

"#]],
    );

    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(fixture.project_path("/src/Overlay.sol")),
        Some(Rope::from("contract Overlay {}")),
        Some(1),
    );
    state.recompute_after_opening_source(Vec::new());

    fixture.check_completions_in(
        &mut state,
        &["$1"],
        str![[r#"
./Existing.sol File filter="./Existing.sol" edit=0:8-0:10
| ./Existing.sol
./Main.sol File filter="./Main.sol" edit=0:8-0:10
| ./Main.sol
./Overlay.sol File filter="./Overlay.sol" edit=0:8-0:10
| ./Overlay.sol

"#]],
    );
}

#[test]
fn completes_relative_import_paths() {
    let mut fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "./$1";
        import {
            Target
        } from './T$2';
        import ".\x2fTar$3";
        import "./Tar$4";
        import "./nested/$6\
        Tar$5get.sol";
        import $7"./Target.sol";
        import './Owner$8';
        /* 😀 */ import "./Token$9";
        import "./Dep$10";
        import ".\x$11";
        contract Main {}

        //- /src/Target.sol
        contract Target {}

        //- /src/nested/Target.sol
        contract Target {}

        //- /src/Owner'sToken.sol
        contract Token {}

        //- /src/Token😀.sol
        contract Token {}

        //- /src/Dependency
        contract Dependency {}

        //- /src/Dependency.md
        not Solidity
        "#,
        "/src/Main.sol",
    );
    fixture.set_open_file_contents("/src/Unsaved.sol", "contract Unsaved {}");
    fixture.set_open_file_contents("/src/DependencyOverlay", "contract DependencyOverlay {}");

    fixture.check_completions(
        &["$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10", "$11"],
        str![[r#"
$1:
./Dependency File filter="./Dependency" edit=0:8-0:10
| ./Dependency
./DependencyOverlay File filter="./DependencyOverlay" edit=0:8-0:10
| ./DependencyOverlay
./Main.sol File filter="./Main.sol" edit=0:8-0:10
| ./Main.sol
./Owner'sToken.sol File filter="./Owner'sToken.sol" edit=0:8-0:10
| ./Owner'sToken.sol
./Target.sol File filter="./Target.sol" edit=0:8-0:10
| ./Target.sol
./Token😀.sol File filter="./Token\\xF0\\x9F\\x98\\x80.sol" edit=0:8-0:10
| ./Token\xF0\x9F\x98\x80.sol
./Unsaved.sol File filter="./Unsaved.sol" edit=0:8-0:10
| ./Unsaved.sol
./nested/ Folder filter="./nested/" edit=0:8-0:10
| ./nested/
$2:
./Target.sol File filter="./Target.sol" edit=3:8-3:11
| ./Target.sol
./Token😀.sol File filter="./Token\\xF0\\x9F\\x98\\x80.sol" edit=3:8-3:11
| ./Token\xF0\x9F\x98\x80.sol
$3:
./Target.sol File filter=".\\x2fTarget.sol" edit=4:8-4:16
| ./Target.sol
$4:
./Target.sol File filter="./Target.sol" edit=5:8-5:13
| ./Target.sol
$5:
./nested/Target.sol File filter="./nested/Target.sol" additional=6:8-7:0="" edit=7:0-7:10
| ./nested/Target.sol
$6:
./nested/Target.sol File filter="./nested/Target.sol" additional=6:18-7:10="" edit=6:8-6:18
| ./nested/Target.sol
$7 $11:
$8:
./Owner'sToken.sol File filter="./Owner\\'sToken.sol" edit=9:8-9:15
| ./Owner\'sToken.sol
$9:
./Token😀.sol File filter="./Token\\xF0\\x9F\\x98\\x80.sol" edit=10:17-10:24
| ./Token\xF0\x9F\x98\x80.sol
$10:
./Dependency File filter="./Dependency" edit=11:8-11:13
| ./Dependency
./DependencyOverlay File filter="./DependencyOverlay" edit=11:8-11:13
| ./DependencyOverlay

"#]],
    );
}

#[test]
fn completes_unterminated_imports_without_replacing_later_lines() {
    for (source, expected) in [
        (
            "import \"./De$1p",
            str![[r#"
./Dependency.sol File filter="./Dependency.sol" edit=0:8-0:13
| ./Dependency.sol

"#]],
        ),
        ("import \"./Dep$1\ncontract Victim {}\n\";", str![]),
        (
            "import \"./Dep$1\ncontract Main { string value = \"ordinary\"; }",
            str![[r#"
./Dependency.sol File filter="./Dependency.sol" edit=0:8-0:13
| ./Dependency.sol

"#]],
        ),
    ] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                "//- /foundry.toml\n//- /src/Main.sol open\n{source}\n\
                 //- /src/Dependency.sol\ncontract Dependency {{}}\n"
            ),
            "/src/Main.sol",
        );
        fixture.check_completions(&["$1"], expected);
    }
}

#[test]
fn import_completion_uses_lsp_lines_after_a_standalone_carriage_return() {
    let mut fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        contract C {}

        //- /src/Dependency.sol
        contract Dependency {}
        "#,
        "/src/Main.sol",
    );
    fixture.set_open_file_contents("/src/Main.sol", "\rcontract C {}\nimport \"./Dep");
    let uri = Url::from_file_path(fixture.project_path("/src/Main.sol")).unwrap();

    check_completions_at(
        &mut fixture.state(),
        [(1, 13), (2, 13)].map(|(line, character)| {
            (format!("{line}:{character}"), uri.clone(), Position::new(line, character), None)
        }),
        str![[r#"
1:13:
2:13:
./Dependency.sol File filter="./Dependency.sol" edit=2:8-2:13
| ./Dependency.sol

"#]],
    );
}

#[test]
fn completes_configured_package_and_remapped_imports() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        libs = ["lib", "node_modules"]
        remappings = [
            "foo/=lib/global/",
            "src:@openzeppelin/=lib/openzeppelin-contracts/contracts/",
            "src:Alias=lib/Alias.sol",
            "src:Extensionless=lib/Extensionless",
            "test:@openzeppelin-test/=lib/openzeppelin-test/contracts/",
            "test:foo/bar/=lib/foo-bar/",
        ]

        //- /src/Main.sol open
        import "@o$1";
        import "A$2";
        import "foo/b$3";
        import "@scope/package/$4";
        import "Ext$5";

        //- /lib/Alias.sol
        contract Alias {}

        //- /lib/Extensionless
        contract Extensionless {}

        //- /lib/global/Other.sol
        contract Other {}

        //- /lib/openzeppelin-contracts/contracts/Token.sol
        contract Token {}

        //- /lib/openzeppelin-test/contracts/TestToken.sol
        contract TestToken {}

        //- /node_modules/@scope/package/Token.sol
        contract Token {}
        "#,
        "/src/Main.sol",
    );

    fixture.check_completions(
        &["$1", "$2", "$3", "$4", "$5"],
        str![[r#"
$1:
@openzeppelin/ Folder filter="@openzeppelin/" edit=0:8-0:10
| @openzeppelin/
$2:
Alias File filter="Alias" edit=1:8-1:9
| Alias
Alias.sol File filter="Alias.sol" edit=1:8-1:9
| Alias.sol
$3:
$4:
@scope/package/Token.sol File filter="@scope/package/Token.sol" edit=3:8-3:23
| @scope/package/Token.sol
$5:
Extensionless File filter="Extensionless" edit=4:8-4:11
| Extensionless

"#]],
    );
}

#[test]
fn completes_remapped_imports_from_the_deepest_foundry_workspace() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/outer/"]

        //- /lib/outer/Outer.sol
        contract Outer {}

        //- /packages/app/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/inner/"]

        //- /packages/app/src/Main.sol open
        import "pkg/$1";

        //- /packages/app/lib/inner/Inner.sol
        contract Inner {}
        "#,
        "/packages/app/src/Main.sol",
    );

    fixture.check_triggered_completions(
        &[("$1", "/")],
        str![[r#"
pkg/Inner.sol File filter="pkg/Inner.sol" edit=0:8-0:12
| pkg/Inner.sol

"#]],
    );
}

#[test]
fn quote_trigger_completes_imports_but_not_ordinary_strings() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /Main.sol open
        import "$1";
        import '$3';
        contract Main {
            string value = "$2";
            string singleQuoted = '$4';
        }

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );

    fixture.check_triggered_completions(
        &[("$1", "\""), ("$2", "\""), ("$3", "'"), ("$4", "'")],
        str![[r#"
$1:
Main.sol File filter="Main.sol" edit=0:8-0:8
| Main.sol
Target.sol File filter="Target.sol" edit=0:8-0:8
| Target.sol
$2 $4:
$3:
Main.sol File filter="Main.sol" edit=1:8-1:8
| Main.sol
Target.sol File filter="Target.sol" edit=1:8-1:8
| Target.sol

"#]],
    );
}

#[test]
fn unowned_import_completion_does_not_fall_back_to_symbols() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /owned/foundry.toml

        //- /unowned/Main.sol open
        import "./$1";
        contract Main {}
        "#,
        "/unowned/Main.sol",
    );

    fixture.check_completions(&["$1"], str![""]);
}
