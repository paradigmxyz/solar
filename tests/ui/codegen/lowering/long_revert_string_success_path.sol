//@ compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ filecheck:
//@ run-call: transfer 0x000000000000000000000000000000000000dEaD, 0 =>
//@ run-call-fail: transfer 0x000000000000000000000000000000000000dEaD, 1 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002645524332303a207472616e7366657220616d6f756e7420657863656564732062616c616e63650000000000000000000000000000000000000000000000000000

// The message is longer than one word, yet the success path of `transfer` neither reads the
// free-memory pointer nor stores any part of it.
contract LongRevertStringSuccessPath {
    mapping(address => uint256) bal;

    // CHECK-LABEL: @module LongRevertStringSuccessPath_runtime
    // CHECK-NOT: push 0x45524332303a
    // CHECK-NOT: mload
    // CHECK: stop
    // CHECK: [cold]:
    // CHECK: push 64
    // CHECK-NEXT: mload
    // CHECK: push 0x45524332303a
    function transfer(address to, uint256 amount) external {
        uint256 b = bal[msg.sender];
        require(b >= amount, "ERC20: transfer amount exceeds balance");
        bal[msg.sender] = b - amount;
        bal[to] += amount;
    }
}
