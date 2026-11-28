//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

pragma abicoder v1;
//~[0_8]^ WARN: ABI coder v1 is not supported
//~[0_9]^^ ERROR: expected `pragma abicoder v2`
