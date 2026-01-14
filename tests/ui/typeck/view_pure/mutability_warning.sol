//@compile-flags: -Ztypeck
// Test: overly permissive mutability warning (2018)

contract C {
    uint256 public x;

    // WARN: view function that could be pure
    function overlyPermissiveView() public view returns (uint256) {
    //~^ WARN: function state mutability can be restricted to pure
        return 42;
    }

    // WARN: nonpayable function that could be view
    function overlyPermissiveNonPayable() public returns (uint256) {
    //~^ WARN: function state mutability can be restricted to view
        return x;
    }
}
