//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

pragma experimental ABIEncoderV2;
//~[0_9]^ ERROR: the experimental ABIEncoderV2 pragma has been removed
