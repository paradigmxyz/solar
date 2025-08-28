//@ revisions: short long
//@[short] compile-flags: -h
//@[long] compile-flags: --help
//@ normalize-stdout-test: "\[default: \d+\]" -> "[default: <DEFAULT>]"
//@ normalize-stdout-test: "\.exe" -> ""
