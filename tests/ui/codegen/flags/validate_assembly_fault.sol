//@ revisions: label width
//@[label] compile-flags: -Zvalidate-assembly -Zvalidate-assembly-fault=label --emit=bin-runtime
//@[width] compile-flags: -Zvalidate-assembly -Zvalidate-assembly-fault=width --emit=bin-runtime
// A fault planted in the bytecode the checker reads, a label push off by one or a byte wider,
// moves a pushed label off its `JUMPDEST` and is reported, in the runtime and the deployment code.
//~? ERROR: assembly validation failed
//~? ERROR: assembly validation failed

contract C {
    function f(uint256 x) external pure returns (uint256 y) {
        for (uint256 i; i < x; i++) y += i;
    }
}
