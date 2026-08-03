//@ revisions: gas size runtime
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@[runtime] compile-flags: -O gas
//@[runtime] run-call: run 41 => 42

contract InternalCallStackReturn {
    // Gas mode keeps a one-word helper result on the physical stack and removes its frame slot.
    // Size mode retains the shared memory convention.
    //
    // GAS-LABEL: @module runtime
    // GAS-NEXT: bb0:
    // GAS-NEXT: push 352
    // GAS: add
    // GAS-NEXT: swap1
    // GAS-NEXT: jump
    //
    // SIZE-LABEL: @module runtime
    // SIZE-NEXT: bb0:
    // SIZE-NEXT: push 384
    // SIZE: add
    // SIZE-NEXT: push [[RETURN_SLOT:[0-9]+]]
    // SIZE-NEXT: mstore
    // SIZE-NEXT: jump
    // SIZE: push [[RETURN_SLOT]]
    // SIZE-NEXT: mload
    function run(uint256 x) external pure returns (uint256) {
        return plusOne(x);
    }

    function plusOne(uint256 x) internal pure returns (uint256) {
        unchecked {
            return x + 1;
        }
    }
}
