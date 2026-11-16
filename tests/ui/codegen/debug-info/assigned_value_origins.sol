//@ compile-flags: -Onone --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A constant or a copy has no instruction at its assignment, so the push or `DUP` that first reads
// it, in the block of the assignment, takes the assignment's origin. Only that one: the push that
// the overflow check repeats keeps the origin of its reader.
contract C {
    // CHECK-LABEL: ; function constantBase(
    // CHECK: push 0x3e8 !metadata(span=[[#BASE:]]..
    // CHECK-SAME: [[#BASE+20]])
    // CHECK: add !metadata(span=[[#SUM:]]..
    // CHECK-SAME: [[#SUM+8]])
    // CHECK-NEXT: push 0x3e8 !metadata(span=[[#SUM]]..[[#SUM+8]])
    function constantBase(uint256 n) external pure returns (uint256) {
        uint256 base = 1000;
        return base + n;
    }

    // CHECK-LABEL: ; function copyOfLength(
    // CHECK: push 2 !metadata(span=[[#PRODUCT:]]..
    // CHECK-SAME: [[#PRODUCT+10]])
    // CHECK-NEXT: dup 2 !metadata(span=[[#LENGTH:]]..
    // CHECK-SAME: [[#LENGTH+30]])
    // CHECK-NEXT: mul !metadata(span=[[#PRODUCT]]..[[#PRODUCT+10]])
    function copyOfLength(uint256[] calldata items) external pure returns (uint256) {
        uint256 length = items.length;
        return length * 2;
    }

    // The first read of `y` is in the arm, which runs only when `c` holds, so its push keeps the
    // origin of the read.
    // CHECK-LABEL: ; function readInArm(
    // CHECK: push 77 !metadata(span=[[#ARM:]]..
    // CHECK-SAME: [[#ARM+5]])
    function readInArm(bool c, uint256 a) external pure returns (uint256) {
        uint256 y = 77;
        if (c) {
            a = a + y;
        }
        return a;
    }

    // `2 ** 128` folds to one word, which takes the span of the expression.
    // CHECK-LABEL: ; function folded(
    // CHECK: push 0x100000000000000000000000000000000 !metadata(span=[[#POWER:]]..
    // CHECK-SAME: [[#POWER+8]])
    // CHECK-NEXT: dup 2 !metadata(span=[[#SUM:]]..
    // CHECK-SAME: [[#SUM+12]])
    function folded(uint256 x) external pure returns (uint256) {
        return x + 2 ** 128;
    }
}

// Two invocations of a modifier lower its body twice from one source statement. Each lowering is
// its own assignment, so both copies of `x` take the declaration's origin.
contract Twice {
    uint256 s;

    // CHECK-LABEL: ; function f(
    // CHECK: dup 1 !metadata(span=[[#TAKE:]]..
    // CHECK-SAME: [[#TAKE+4]])
    // CHECK: dup 2 !metadata(span=[[#DECL:]]..
    // CHECK-SAME: [[#DECL+14]])
    // CHECK: dup 1 !metadata(span=[[#DECL]]..
    modifier m(bool take, uint256 x) {
        if (take) {
            uint256 y = x;
            s = y;
        }
        _;
    }

    function f(bool c, uint256 x) external m(c, x) m(!c, x) returns (uint256) {
        return x;
    }
}
