//@ revisions: none gas
//@[none] compile-flags: -Onone
//@[none] filecheck: --check-prefixes=CHECK,NONE
//@[gas] compile-flags: -Ogas
//@[gas] filecheck: --check-prefixes=CHECK,GAS
//@ compile-flags: --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ normalize-stdout-test: "(?s).+" -> ""

// A public function that another function calls has an external wrapper that calls its extracted
// body. The wrapper exits after the body returns, and like the epilogue of a clang function
// (`CodeGenFunction::FinishFunction(EndLoc)`) that exit maps to the closing brace of the function,
// not to its header, which a debugger that steps by line would show again after the last
// statement. The entry of the wrapper keeps the whole definition, and the call of the wrapper
// keeps it as well, so the header is the first stop. The return tag after a call from the source
// stays under the call, and so does the one after a call that decodes a memory argument, which is
// part of the entry of the wrapper.
struct Point {
    uint256 x;
    uint256 y;
}

contract C {
    uint256 stored;

    // CHECK-LABEL: [entry, invoke=add@
    // CHECK-SAME: [[#ADD_LO:]]..[[#ADD_HI:]]]
    // CHECK-SAME: !metadata(span=[[#ADD_LO]]..
    // CHECK-SAME: [[#ADD_HI]])
    // NONE: [continuation] !metadata(span=[[#ADD_HI - 1]]..
    // NONE-SAME: [[#ADD_HI]])
    // CHECK: return !metadata(span=[[#ADD_HI - 1]]..
    // CHECK-SAME: [[#ADD_HI]], exit=return)
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    // CHECK-LABEL: [entry, invoke=set@
    // CHECK-SAME: [[#SET_LO:]]..[[#SET_HI:]]]
    // CHECK-SAME: !metadata(span=[[#SET_LO]]..
    // CHECK-SAME: [[#SET_HI]])
    // NONE: [continuation] !metadata(span=[[#SET_HI - 1]]..
    // NONE-SAME: [[#SET_HI]])
    // CHECK: stop !metadata(span=[[#SET_HI - 1]]..
    // CHECK-SAME: [[#SET_HI]], exit=return)
    function set(uint256 a) public {
        stored = a;
    }

    // CHECK-LABEL: ; function use_add(
    // CHECK: [continuation] !metadata(span=[[#CALL:]]..
    // CHECK-SAME: [[#CALL + 9]])
    // CHECK: return !metadata(span=[[#CALL - 7]]..
    // CHECK-SAME: [[#CALL + 10]], exit=return)
    function use_add(uint256 a) external pure returns (uint256) {
        return add(a, 1);
    }

    function use_set(uint256 a) external {
        set(a);
        set(a + 1);
    }

    // At `-Ogas` the wrapper decodes the struct in a helper before it calls the body.
    // CHECK-LABEL: [entry, invoke=sum@
    // CHECK-SAME: [[#SUM_LO:]]..[[#SUM_HI:]]]
    // CHECK-SAME: !metadata(span=[[#SUM_LO]]..
    // CHECK-SAME: [[#SUM_HI]])
    // GAS: [continuation] !metadata(span=[[#SUM_LO]]..
    // GAS-SAME: [[#SUM_HI]])
    // GAS: [continuation] !metadata(span=[[#SUM_HI - 1]]..
    // GAS-SAME: [[#SUM_HI]])
    // CHECK: return !metadata(span=[[#SUM_HI - 1]]..
    // CHECK-SAME: [[#SUM_HI]], exit=return)
    function sum(Point memory p) public pure returns (uint256) {
        return p.x + p.y;
    }

    function use_sum(Point memory p) external pure returns (uint256) {
        return sum(p) + 1;
    }

    // The reads of the results of a body that returns a tuple are part of the epilogue too.
    // CHECK-LABEL: [entry, invoke=pair@
    // CHECK-SAME: [[#PAIR_LO:]]..[[#PAIR_HI:]]]
    // CHECK-SAME: !metadata(span=[[#PAIR_LO]]..
    // CHECK-SAME: [[#PAIR_HI]])
    // NONE: [continuation] !metadata(span=[[#PAIR_HI - 1]]..
    // NONE-SAME: [[#PAIR_HI]])
    // CHECK: return !metadata(span=[[#PAIR_HI - 1]]..
    // CHECK-SAME: [[#PAIR_HI]], exit=return)
    function pair(uint256 a) public pure returns (uint256, uint256) {
        return (a, a + 1);
    }

    function use_pair(uint256 a) external pure returns (uint256) {
        (uint256 x,) = pair(a);
        return x;
    }
}
