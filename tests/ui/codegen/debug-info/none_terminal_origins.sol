//@ compile-flags: -Onone --emit=srcmap-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// An unoptimized build keeps equal terminal blocks apart. Sharing one merges the origins of its
// copies, which the legacy source map writes as -1, so a merged `return true;` or an encoding
// return would have no instruction to anchor to.

// CHECK-LABEL: "srcmap-runtime"
// CHECK-NOT: -1:-1:-1
interface I {
    function g(uint256 x, address y) external;
}

contract C {
    function f(uint256 a, bool b) external pure returns (bool) {
        if (b) {
            if (a == 1) {
                return true;
            }
            return false;
        }
        if (a == 2) {
            return true;
        }
        return false;
    }

    function h(uint256 a, bool b) external view returns (bytes memory) {
        if (b) {
            return abi.encodeCall(I.g, (a, msg.sender));
        }
        if (a == 2) {
            return abi.encodeCall(I.g, (a, msg.sender));
        }
        return "";
    }
}
