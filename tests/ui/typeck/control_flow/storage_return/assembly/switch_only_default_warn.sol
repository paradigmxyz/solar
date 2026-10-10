// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/assembly/switch_only_default_warn.sol
contract C {
    struct S { bool f; }
    S s;
    function f(uint256 a) internal pure returns (S storage c) {
        assembly {
            switch a //~ WARN: `switch` statement has only a default case
                default { c.slot := s.slot }
        }
    }
}
