//@ revisions: evm sonatina sonatina_none sonatina_size
//@ compile-flags: --evm-version osaka
//@[evm] compile-flags: --codegen-backend evm -Ogas
//@[sonatina] compile-flags: --codegen-backend sonatina -Ogas
//@[sonatina_none] compile-flags: --codegen-backend sonatina -Onone
//@[sonatina_size] compile-flags: --codegen-backend sonatina -Osize
//@ run-call: decrement 0 => 0
//@ run-call: decrement 1 => 0
//@ run-call: decrement 10 => 0

contract Recursion {
    function decrement(uint256 input) public pure returns (uint256) {
        if (input == 0) return 0;
        unchecked { return decrement(input - 1); }
    }
}
