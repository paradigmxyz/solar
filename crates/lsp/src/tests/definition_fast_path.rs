use super::*;
use snapbox::{assert_data_eq, str};

#[test]
fn current_open_code_and_import_literals_keep_distinct_targets() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol open
        import "./$1Target.sol";
        contract Main {
            function target() internal {}
            function caller() external { $2target(); }
        }

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );

    fixture.check_queries(
        &[Query::Definition],
        [1, 2],
        str![[r#"
$1 /Target.sol:0:0 contract Target {}
$2 /Main.sol:2:13 function target() internal {}

"#]],
    );
}

#[tokio::test(flavor = "current_thread")]
async fn changed_imports_do_not_use_an_old_code_symbol() {
    let import = "import \"./Target.sol\";";
    for (open, expected) in [(true, "<none>\n"), (false, "/Target.sol:0:0 contract Target {}\n")] {
        let fixture = RequestFixture::new(
            &format!(
                r#"
                //- /Main.sol{}
                contract $1Main {{}}

                //- /Target.sol
                contract Target {{}}
                "#,
                if open { " open" } else { "" },
            ),
            "/Main.sol",
        );
        let mut state = fixture.state();
        let main = fixture.project_path("/Main.sol");
        if open {
            set_overlay(&state, &main, import, None);
        } else {
            std::fs::write(main, import).unwrap();
        }

        assert_data_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, expected);
    }
}
