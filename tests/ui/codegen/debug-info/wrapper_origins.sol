//@ revisions: runtime creation
//@ compile-flags: -Ogas --emit=srcmap
//@[runtime] compile-flags: -Zdump=evm-ir-runtime-scheduled
//@[runtime] filecheck: --check-prefix=RUNTIME
//@[creation] compile-flags: -Zdump=evm-ir
//@[creation] filecheck: --check-prefix=CREATION
//@ normalize-stdout-test: "(?s).+" -> ""

// An external function's wrapper belongs to its declaration, as in solc's `appendFunctionSelector`,
// which emits the callvalue check, the head size and validation checks with their reverts, the
// head loads under `LocationSetter(functionType->declaration())`. That is the function definition,
// or the variable of a public getter. The constructor's argument decoding belongs to the
// constructor, as in `appendConstructor`. The body keeps its own origins, and the encoding of a
// `return` keeps the origin of the statement it replaces.

// RUNTIME-LABEL: (scheduled runtime) ===
// RUNTIME: [entry, invoke=total@[[GETTER:[0-9]+..[0-9]+]]]
// RUNTIME: callvalue !metadata(span=[[GETTER]])
// RUNTIME: jumpi !metadata(span=[[GETTER]])
// RUNTIME: return !metadata(span=[[GETTER]], exit=return)
// RUNTIME: [entry, invoke=f@[[F:[0-9]+..[0-9]+]]]
// RUNTIME: calldatasize !metadata(span=[[F]])
// RUNTIME: lt !metadata(span=[[F]])
// RUNTIME: callvalue !metadata(span=[[F]])
// RUNTIME: calldataload !metadata(span=[[F]])
// RUNTIME: jumpi !metadata(span=[[F]])
// RUNTIME: return !metadata(span={{[0-9]+..[0-9]+}}, exit=return)
// RUNTIME: revert !metadata(span=[[F]], exit=revert)

// CREATION-LABEL: (creation) ===
// CREATION: bb0 [dispatch, invoke=constructor@[[CTOR:[0-9]+..[0-9]+]]]
// CREATION: lt !metadata(span=[[CTOR]])
// CREATION: callvalue !metadata(span=[[CTOR]])
// CREATION: jumpi !metadata(span=[[CTOR]])
// CREATION: mload !metadata(span=[[CTOR]])
contract C {
    uint256 public total;

    constructor(uint256 a, bytes memory b) {
        total = a + b.length;
    }

    function f(uint256 a, uint8 b, bytes calldata c) external returns (uint256) {
        total += a + b + c.length;
        return total;
    }

    function g() external payable {}
}
