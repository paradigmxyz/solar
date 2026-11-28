//@ revisions: 0_8 0_9 wrapper
//@[0_9] compile-flags: --solc-version 0.9.0
//@[wrapper] rustc-env: SOLC_WRAPPER=1 SOLC_WRAPPER_VERSION=0.9.0
// `SOLC_WRAPPER_VERSION` selects the rules when it names a supported version.

pragma solidity ^0.8.0;
//~[0_9,wrapper]^ WARN: source file requires different compiler version
pragma solidity ^0.9.0;
//~[0_8]^ WARN: source file requires different compiler version
