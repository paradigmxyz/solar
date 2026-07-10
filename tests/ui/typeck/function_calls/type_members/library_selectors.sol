//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/libraries/library_function_selectors.sol

library L {
    function f(uint256 x) external returns (uint256) { return x; }
    function g(uint256[] storage s) external returns (uint256) { return s.length; }
    function h(uint256[] memory m) public returns (uint256) { return m.length; }
}
contract C {
    uint256[] s;

    constructor() {
        while (s.length < 42) s.push(0);
    }

    function f() public returns (bool, bool, uint256) {
        (bool success, bytes memory data) =
            address(L).delegatecall(abi.encodeWithSelector(L.f.selector, 7));
        return (L.f.selector == bytes4(keccak256("f(uint256)")), success, abi.decode(data, (uint256)));
    }

    function g() public returns (bool, bool, uint256) {
        uint256 sPtr;
        assembly { sPtr := s.slot }
        (bool success, bytes memory data) =
            address(L).delegatecall(abi.encodeWithSelector(L.g.selector, sPtr));
        return (
            L.g.selector == bytes4(keccak256("g(uint256[] storage)")),
            success,
            abi.decode(data, (uint256))
        );
    }

    function h() public returns (bool, bool, uint256) {
        (bool success, bytes memory data) = address(L).delegatecall(
            abi.encodeWithSelector(L.h.selector, new uint256[](23))
        );
        return (L.h.selector == bytes4(keccak256("h(uint256[])")), success, abi.decode(data, (uint256)));
    }
}
