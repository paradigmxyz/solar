//@ codegen-matrix: standard
//@ run-call: constructorPositional => 9
//@ run-call: constructorNamedLast => 9
//@ run-call: constructorNamedFirst => 9
//@ run-call: declaration => 9
//@ run-call: returnTuple => 9
//@ run-call: memoryPlace => 9
//@ run-call: arrayLiteral => 1
// A storage array operand of a struct constructor, tuple, or assignment to a memory place is
// copied to memory after the operands that follow it ran, as solc does via IR. An array literal
// copies each element when it is evaluated.
struct P {
    uint256[] a;
    uint256 b;
}

contract Test {
    uint256[] sa;

    constructor() {
        sa.push(1);
    }

    function w() internal returns (uint256) {
        sa[0] = 9;
        return 0;
    }

    function wArray() internal returns (uint256[] memory) {
        sa[0] = 9;
        return new uint256[](1);
    }

    function pair() internal returns (uint256[] memory, uint256) {
        return (sa, w());
    }

    function constructorPositional() external returns (uint256) {
        P memory p = P(sa, w());
        return p.a[0];
    }

    function constructorNamedLast() external returns (uint256) {
        P memory p = P({b: w(), a: sa});
        return p.a[0];
    }

    function constructorNamedFirst() external returns (uint256) {
        P memory p = P({a: sa, b: w()});
        return p.a[0];
    }

    function declaration() external returns (uint256) {
        (uint256[] memory m, uint256 z) = (sa, w());
        return m[0] + z;
    }

    function returnTuple() external returns (uint256) {
        (uint256[] memory m, uint256 z) = pair();
        return m[0] + z;
    }

    function memoryPlace() external returns (uint256) {
        uint256[][] memory mm = new uint256[][](1);
        mm[w()] = sa;
        return mm[0][0];
    }

    function arrayLiteral() external returns (uint256) {
        uint256[][2] memory m = [sa, wArray()];
        return m[0][0];
    }
}
