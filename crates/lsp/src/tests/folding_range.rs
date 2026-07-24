use super::support::RequestFixture;
use lsp_types::Url;
use snapbox::str;

#[test]
fn returns_syntax_ranges_without_waiting_for_analysis() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Folding.sol open
        import "a.sol";
        import "b.sol";
        contract C {
            // first
            // second
            function f() external {
                if (true) {
                }
            }
        }
        "#,
        "/Folding.sol",
    );

    fixture.check_folding_ranges_while_analysis_pending(
        "/Folding.sol",
        str![[r#"
0:0-1:15 kind=imports collapsed_text=None
2:0-9:1 kind=code collapsed_text=None
3:4-4:13 kind=comment collapsed_text=None
5:4-8:5 kind=code collapsed_text=None
6:18-7:9 kind=code collapsed_text=None

"#]],
    );
}

#[test]
fn prefers_open_vfs_contents_over_stale_disk() {
    let fixture = RequestFixture::new(
        r#"
        //- /Open.sol open
        contract Open {
            uint256 value;
        }
        "#,
        "/Open.sol",
    );
    fixture.write_file("/Open.sol", "contract Disk {}");

    fixture.check_folding_ranges(
        "/Open.sol",
        str![[r#"
0:0-2:1 kind=code collapsed_text=None

"#]],
    );
}

#[test]
fn reads_closed_documents_from_disk() {
    let fixture = RequestFixture::new(
        r#"
        //- /Disk.sol
        contract Disk {
            uint256 value;
        }
        "#,
        "/Disk.sol",
    );

    fixture.check_folding_ranges(
        "/Disk.sol",
        str![[r#"
0:0-2:1 kind=code collapsed_text=None

"#]],
    );
}

#[test]
fn parses_folding_ranges_on_the_blocking_pool() {
    let fixture = RequestFixture::new(
        r#"
        //- /Blocking.sol open
        contract Blocking {
        }
        "#,
        "/Blocking.sol",
    );

    fixture.check_folding_range_uses_blocking_pool(
        "/Blocking.sol",
        str![[r#"
0:0-1:1 kind=code collapsed_text=None

"#]],
    );
}

#[test]
fn distinguishes_empty_documents_from_unavailable_documents() {
    let fixture = RequestFixture::new(
        r#"
        //- /Empty.sol open
        "#,
        "/Empty.sol",
    );

    fixture.check_folding_ranges("/Empty.sol", str![""]);
    fixture.check_folding_range_returns_none(Url::parse("untitled:Folding.sol").unwrap());
    fixture.check_missing_folding_range_returns_none("/Missing.sol");
}
