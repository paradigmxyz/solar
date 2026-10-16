//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

enum E { A //~ ERROR: expected one of `,` or `}`, found `<eof>`
