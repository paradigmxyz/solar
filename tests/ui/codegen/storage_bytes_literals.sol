//@compile-flags: -Zcodegen --emit=mir

contract StorageBytesLiterals {
    string public greeting = "hi";
    string public longGreeting = "abcdefghijklmnopqrstuvwxyzABCDEF";
    bytes public blob = hex"aabbcc";
    bytes public longBlob = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    mapping(uint256 => string) public names;
    mapping(uint256 => bytes) public blobs;

    function assignState() public {
        greeting = "bye";
        longGreeting = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef";
        blob = hex"01020304";
        longBlob = hex"202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
    }

    function assignMapping() public {
        names[1] = "alice";
        blobs[2] = hex"deadbeef";
    }
}
