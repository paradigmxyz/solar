//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: pointer => 1, 0
//@ run-call: twoArrays => 2, 0
//@ run-call: threeArrays => 2, 2, 11

// An early `return` in an inner invocation leaves the outer invocation's storage pointer local
// and storage parameter alone.
contract NestedSameStorage {
    struct S {
        uint256 v;
    }

    S a;
    S b;
    uint256[] arrA;
    uint256[] arrB;

    modifier sp(bool first) {
        S storage p = first ? a : b;
        if (!first) return;
        _;
        p.v += 1;
    }

    modifier push(uint256[] storage p, uint256 v) {
        if (v == 2) return;
        p.push(v);
        _;
        p.push(v + 10);
    }

    function pointer() external returns (uint256, uint256) {
        pointer1();
        return (a.v, b.v);
    }

    function twoArrays() external returns (uint256, uint256) {
        twoArrays1();
        return (arrA.length, arrB.length);
    }

    function threeArrays() external returns (uint256, uint256, uint256) {
        threeArrays1();
        return (arrA.length, arrB.length, arrA[arrA.length - 1]);
    }

    function pointer1() internal sp(true) sp(false) {}

    function twoArrays1() internal push(arrA, 1) push(arrB, 2) {}

    function threeArrays1() internal push(arrA, 1) push(arrB, 3) push(arrA, 2) {}
}
