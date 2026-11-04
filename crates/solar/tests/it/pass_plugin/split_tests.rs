use super::*;

#[test]
fn splitting_a_predecessor_repairs_its_successors_phis() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.mir");
    fs::write(&path, "@module Split\n@phase lowered\nfn @entry() [entry] {\nbb0:\n v0 = calldataload 0\n v1 = eq v0, 0\n jumpi v1, bb1, bb2\nbb1:\n v2 = phi [bb0: 111], [bb2: 222]\n mstore 0, v2\n returndata 0, 32\nbb2:\n jump bb1\n}\n").unwrap();
    let output = run(
        &path,
        &sample(),
        &["-Ogas", "-Zmir-pipeline=example::split-lowered-block", "-Zprint-after-each"],
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let program = &text[text.find("@module").unwrap()..];
    for (input, expected) in [(0, 111), (1, 222)] {
        let input = alloy_primitives::U256::from(input).to_be_bytes::<32>();
        let expected = alloy_primitives::U256::from(expected).to_be_bytes::<32>().to_vec();
        let result =
            solar_mir_interpreter::transact(program, &input, &mut EmptyHost, Default::default())
                .unwrap();
        assert_eq!(result.outcome, solar_mir_interpreter::Outcome::Success(expected));
    }
    assert_data_eq!(
        text.replace(&path.display().to_string(), "$INPUT"),
        str![[r#"
; === $INPUT (after example::split-lowered-block) ===
@module Split
@phase lowered
; runtime dispatch entry
fn @entry() [entry] {
  bb0:
    jump bb3
  bb3:                                            ; preds = bb0
    v0 = calldataload 0
    v1 = eq v0, 0
    jumpi v1, bb1, bb2
  bb2:                                            ; preds = bb3
    jump bb1
  bb1:                                            ; preds = bb2, bb3
    v2 = phi [bb3: 111], [bb2: 222]
    mstore 0, v2
    returndata 0, 32
}

"#]]
    );
}
