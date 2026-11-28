//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

pragma solidity ^0.8.0;
//~[0_9]^ WARN: source file requires different compiler version
pragma solidity ^0.9.0;
//~[0_8]^ WARN: source file requires different compiler version
