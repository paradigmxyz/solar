// Finding 43: `delete` on a storage pointer local is accepted and lowered to a clear of the
// pointed-to storage; solc rejects it with error 9767.
//   solc --bin symbolic-audit/delete_storage_pointer.sol
//   target/debug/solar --emit abi symbolic-audit/delete_storage_pointer.sol
contract C {
    uint256[] nums;
    struct S { uint256 a; }
    S s;
    function f() external { uint256[] storage r = nums; delete r; }
    function g() external { S storage p = s; delete p; }
}
