// ported-from: test/libsolidity/syntaxTests/tryCatch/invalid_error_name.sol
// The upstream test writes the clauses with an empty parameter list, which our parser rejects.

contract C {
    function f() public returns (uint256, uint256) {
        try this.f() {
        } catch Error2(string memory) {
            //~^ ERROR: invalid catch clause name
        } catch abc(uint256) {
            //~^ ERROR: invalid catch clause name
        }
    }
}
