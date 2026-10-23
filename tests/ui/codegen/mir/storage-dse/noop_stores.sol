//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: masked 1, 9 => 5
//@ run-call: masked 2, 9 => 0
//@ run-call: branch 1, true => 5
//@ run-call: branch 1, false => 5
//@ run-call: checked 1, 3 => 5
//@ run-call-fail: checked 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: write 1, 9 => 9
//@[mir] run-call: staticTouch => false
//@[gas] run-call: staticTouch => true
//@[size] run-call: staticTouch => true

// Read-modify-writes that change no bit. Once the value simplifies to the word
// just read, the store writes what the slot holds: it goes, and so does the
// load when nothing else reads it. As with solc's optimizer, the dropped store
// no longer reverts in a static context, which `-O none` still shows.
contract NoopStores {
    mapping(uint256 => uint256) m;
    uint256 s;

    constructor() {
        m[1] = 5;
    }

    // CHECK-LABEL: fn @touch()
    // CHECK-NOT: sload
    // CHECK-NOT: sstore
    // CHECK: stop
    function touch(uint256 i, uint256 v) external {
        uint256 x = m[i];
        m[i] = x ^ ((x ^ v) & 0);
    }

    // CHECK-LABEL: fn @masked()
    // CHECK: sload
    // CHECK-NOT: sstore
    // CHECK: returndata
    function masked(uint256 i, uint256 v) external returns (uint256) {
        uint256 x = m[i];
        m[i] = x ^ ((x ^ v) & 0);
        return m[i];
    }

    // CHECK-LABEL: fn @branch()
    // CHECK-NOT: sstore
    // CHECK: returndata
    function branch(uint256 i, bool c) external returns (uint256) {
        uint256 x = m[i];
        if (c) m[i] = x;
        return m[i];
    }

    // The overflow check of the unused sum stays between the load and the store.
    // CHECK-LABEL: fn @checked()
    // CHECK-NOT: sstore
    // CHECK: returndata
    function checked(uint256 i, uint256 a) external returns (uint256) {
        uint256 x = m[i];
        uint256 y = a + 1;
        m[i] = x ^ ((x ^ y) & 0);
        return m[i];
    }

    // CHECK-LABEL: fn @write()
    // CHECK: sstore
    function write(uint256 i, uint256 v) external returns (uint256) {
        uint256 x = m[i];
        m[i] = x ^ ((x ^ v) & type(uint256).max);
        return m[i];
    }

    // Between two gas reads, the accesses stay for the second read to observe.
    // CHECK-LABEL: fn @measured()
    // CHECK: = gas
    // CHECK: sload
    // CHECK: sstore
    // CHECK: = gas
    function measured() external returns (uint256) {
        uint256 g = gasleft();
        uint256 x = s;
        s = x;
        return g - gasleft();
    }

    function staticTouch() external view returns (bool ok) {
        (ok,) = address(this).staticcall(abi.encodeCall(this.touch, (1, 9)));
    }
}
