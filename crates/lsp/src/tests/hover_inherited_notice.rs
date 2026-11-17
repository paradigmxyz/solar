use super::support::{Query, RequestFixture};
use snapbox::str;

// A `///` line without a tag continues the tag before it, so it does not replace the inherited
// notice, as in solc.
#[test]
fn continuation_line_keeps_inherited_notice() {
    let fixture = RequestFixture::new(
        r#"
        //- /Use.sol open
        contract Base {
            /// @notice Base notice.
            function f() public virtual {}

            /// @notice Other notice.
            function g() public virtual {}
        }

        contract Child is Base {
            /// @inheritdoc Base
            /// @custom:note first
            /// continues the custom tag
            function $1f() public override {}

            /// @inheritdoc Base
            /// @notice Own notice.
            /// continues the notice
            function $2g() public override {}
        }
        "#,
        "/Use.sol",
    );

    fixture.check_queries(
        &[Query::Hover],
        1..=2,
        str![[r#"
$1 10:13-10:14 function f() public override

continues the custom tag

Base notice.
$2 14:13-14:14 function g() public override

Own notice.

continues the notice

"#]],
    );
}
