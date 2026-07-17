use super::support::RequestFixture;
use snapbox::str;

#[test]
fn all_import_forms_use_content_only_utf16_ranges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imports.sol
        /* 😀 */ import "./Plain.sol";
        import * as Glob from "./Glob.sol";
        import {Named as Alias} from "./Named.sol";

        //- /Plain.sol
        contract Plain {}

        //- /Glob.sol
        contract Glob {}

        //- /Named.sol
        contract Named {}
        "#,
        "/Imports.sol",
    );

    fixture.check_document_links(
        "/Imports.sol",
        str![[r#"
0:17..0:28 -> /Plain.sol
1:23..1:33 -> /Glob.sol
2:30..2:41 -> /Named.sol

"#]],
    );
}

#[test]
fn returns_only_successfully_resolved_imports() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Imports.sol
        import "./Valid.sol";
        import "./Missing.sol";

        //- /Valid.sol
        contract Valid {}
        "#,
        "/Imports.sol",
    );

    fixture.check_document_links(
        "/Imports.sol",
        str![[r#"
0:8..0:19 -> /Valid.sol

"#]],
    );
}
