//@ codegen-matrix: standard
//@ compile-flags: --no-warnings
//@ run-call: f() => 4, 0xffffffff00000000000000000000000000000000000000000000000000000000

// `abi.encodeWithSelector` cleans a `bytes4` with forged low bits before it stores the selector
// word, so the bytes after the 4-byte result are zero (CODEGEN-033). solc with `--via-ir` stores
// the raw word and returns all `0xff` bytes there.
contract C {
    function f() external pure returns (uint256 length, bytes32 word) {
        bytes4 sel;
        assembly {
            sel := not(0)
        }
        bytes memory r = abi.encodeWithSelector(sel);
        length = r.length;
        assembly {
            word := mload(add(r, 0x20))
        }
    }
}
