//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: balanceOfDirty; value=3 => 3
//@ run-call: callDirty => true
//@ run-call: codeSizeOfDirty => true

// An address with bits above 160 that assembly wrote is cleaned when Solidity reads it, so the
// EVM operands that take a whole word see the real address.
contract AssemblyBoundaryAddressOperands {
    function dirtySelf() internal view returns (address account) {
        assembly { account := or(address(), shl(160, 1)) }
    }

    // CHECK-LABEL: fn @balanceOfDirty{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = icall @dirtySelf
    // CHECK-NEXT: trunc i256 [[WORD]] to i160
    // CHECK: balance
    function balanceOfDirty() external payable returns (uint256) {
        address account = dirtySelf();
        return account.balance;
    }

    function callDirty() external returns (bool success) {
        address account = dirtySelf();
        (success,) = account.call("");
    }

    function codeSizeOfDirty() external view returns (bool) {
        address account = dirtySelf();
        return account.code.length == address(this).code.length && account.code.length != 0;
    }

    receive() external payable {}
}
