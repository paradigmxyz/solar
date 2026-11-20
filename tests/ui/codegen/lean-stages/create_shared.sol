//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// Final assembly packs the program data again once it links the creation code in:
// `SharedChild` and `SharedTwin` compile to the same creation code, which the runtime code then
// holds once. The stages name data by what it holds, not by its number, so both creates copy
// from where assembly put that code.

contract SharedChild {}

contract SharedTwin {}

contract LeanStagesCreateShared {
    function f() external returns (address) {
        return address(new SharedChild());
    }

    function g(bytes32 salt) external returns (address) {
        return address(new SharedTwin{salt: salt}());
    }
}
