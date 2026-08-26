//@ revisions: gas size
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: run() => 1

contract NestedConstructor {
    bytes32 public firstHash;
    bytes32 public secondHash;
    uint256 public lengths;

    constructor(bytes[] memory values) {
        firstHash = keccak256(values[0]);
        secondHash = keccak256(values[1]);
        lengths = (values[0].length << 128) | values[1].length;
    }
}

contract ConstructorNestedAbiBounds {
    function run() external returns (uint256) {
        bytes[] memory values = new bytes[](2);
        values[0] = hex"9e5b51c01b06aa9662e69b102026ffa1757b2a877ea1721d7e523dd900000000000000000000000000000000000000000000000000000000000008a09ff4";
        values[1] = hex"0102030405";
        NestedConstructor child = new NestedConstructor(values);
        bool hashesMatch = child.firstHash() == keccak256(values[0])
            && child.secondHash() == keccak256(values[1]);
        bool lengthsMatch = child.lengths() == ((values[0].length << 128) | values[1].length);
        return hashesMatch && lengthsMatch ? 1 : 0;
    }
}
