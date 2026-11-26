pragma solidity ^0.8.20;
// keccak256 of a packed encoding with bytes in it is a function of its bytes, whether they are
// hashed through a variable or built in place, so a CREATE2 address computed from an init code
// matches a spec that builds the same init code. A packed encoding can split the same bytes
// across its arguments in more than one way, so two such hashes differ only where their bytes
// are known to.
contract Create2Predict {
    /// @custom:ensures predicted == address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(abi.encodePacked(prefix, impl, suffix)))))))
    function ok_predict(bytes memory prefix, address impl, bytes memory suffix, bytes32 salt)
        external
        view
        returns (address predicted)
    {
        bytes memory initCode = abi.encodePacked(prefix, impl, suffix);
        predicted = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(initCode))))));
    }

    /// @custom:ensures predicted == address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(abi.encodePacked(hex"3d602d80600a3d3981f3363d3d373d3d3d363d73", impl, hex"5af43d82803e903d91602b57fd5bf3")))))))
    function ok_predict_clone(address impl, bytes32 salt) external view returns (address predicted) {
        bytes memory initCode =
            abi.encodePacked(hex"3d602d80600a3d3981f3363d3d373d3d3d363d73", impl, hex"5af43d82803e903d91602b57fd5bf3");
        predicted = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(initCode))))));
    }

    /// @custom:ensures predicted == address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xfe), address(this), salt, keccak256(abi.encodePacked(hex"3d602d80600a3d3981f3363d3d373d3d3d363d73", impl, hex"5af43d82803e903d91602b57fd5bf3")))))))
    // Refuting the claim needs a model of the hash axioms, which the solver may not find in
    // time.
    // expect: solver returned unknown
    function bad_prefix_byte(address impl, bytes32 salt) external view returns (address predicted) {
        bytes memory initCode =
            abi.encodePacked(hex"3d602d80600a3d3981f3363d3d373d3d3d363d73", impl, hex"5af43d82803e903d91602b57fd5bf3");
        predicted = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(initCode))))));
    }

    /// @custom:ensures predicted == address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(abi.encodePacked(hex"3d602d80600a3d3981f3363d3d373d3d3d363d73", impl, hex"5af43d82803e903d91602b57fd5bf4")))))))
    // Refuting the claim needs a model of the hash axioms, which the solver may not find in
    // time.
    // expect: solver returned unknown
    function bad_other_init_code(address impl, bytes32 salt) external view returns (address predicted) {
        bytes memory initCode =
            abi.encodePacked(hex"3d602d80600a3d3981f3363d3d373d3d3d363d73", impl, hex"5af43d82803e903d91602b57fd5bf3");
        predicted = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(initCode))))));
    }

    /// @custom:ensures result == (keccak256(a) == keccak256(abi.encodePacked(x, y)))
    function ok_fixed_through_variable(bytes memory a, uint8 x, address y) external pure returns (bool result) {
        bytes memory b = abi.encodePacked(x, y);
        result = keccak256(a) == keccak256(b);
    }

    /// @custom:ensures (a.length + b.length != c.length + d.length) ==> result
    function ok_lengths_differ(bytes memory a, bytes memory b, bytes memory c, bytes memory d)
        external
        pure
        returns (bool result)
    {
        result = keccak256(abi.encodePacked(a, b)) != keccak256(abi.encodePacked(c, d));
    }

    /// @custom:ensures result
    function ok_first_byte_differs(bytes memory a, bytes memory b) external pure returns (bool result) {
        result = keccak256(abi.encodePacked(bytes1(0x01), a)) != keccak256(abi.encodePacked(bytes1(0x02), b));
    }

    // Different splits of the same bytes hash alike.
    /// @custom:ensures (a.length != c.length) ==> result
    // Refuting the claim needs a model of the hash axioms, which the solver may not find in
    // time.
    // expect: solver returned unknown
    function bad_split_distinct(bytes memory a, bytes memory b, bytes memory c, bytes memory d)
        external
        pure
        returns (bool result)
    {
        result = keccak256(abi.encodePacked(a, b)) != keccak256(abi.encodePacked(c, d));
    }

    /// @custom:ensures result
    // Refuting the claim needs a model of the hash axioms, which the solver may not find in
    // time.
    // expect: solver returned unknown
    function bad_literal_split_distinct(bytes memory x, bytes memory y) external pure returns (bool result) {
        result = keccak256(abi.encodePacked(hex"0102", x)) != keccak256(abi.encodePacked(hex"01", y));
    }
}
