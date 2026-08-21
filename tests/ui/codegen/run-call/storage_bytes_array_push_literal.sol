//@ run-call: C::pushLiteral() => 1

// A bare numeric literal pushed into a `bytesN` storage array types from the
// element, so it must store left-aligned; the packed store's high mask
// silently zeroed the right-aligned form.

contract C {
    bytes4[] internal sels;
    bytes2[] internal pairs;

    function pushLiteral() external returns (uint256) {
        sels.push(0x12345678);
        sels.push(0xabcdef01);
        require(sels.length == 2, "len");
        require(sels[0] == 0x12345678, "e0");
        require(sels[1] == 0xabcdef01, "e1");
        pairs.push(0xbeef);
        require(pairs[0] == 0xbeef, "pair");
        sels.pop();
        require(sels.length == 1 && sels[0] == 0x12345678, "pop");
        return 1;
    }
}
