//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

pragma experimental SMTChecker;
//~[0_9]^ ERROR: unsupported experimental feature name
