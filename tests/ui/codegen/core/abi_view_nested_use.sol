//@ compile-flags: -Ogas --emit=bin

// The array and struct views an `abi.decode` declaration makes, and the views
// of their elements and fields, are read in place like any view: any other
// use is an error, and so is a write that may change the bytes they read while
// they are still read.
contract Test {
    struct Pair {
        uint256 a;
        bytes b;
    }

    function keep(bytes memory b) internal pure returns (bytes memory) {
        return b;
    }

    function copied(bytes memory data) public pure returns (uint256) {
        /// @custom:solar-view
        (bytes[] memory items) = abi.decode(data, (bytes[]));
        bytes memory copy = items[0]; //~ ERROR: the view `items` can only be read in place
        return copy.length;
    }

    function passed(bytes memory data) public pure returns (uint256) {
        /// @custom:solar-view
        (Pair memory pair) = abi.decode(data, (Pair));
        return keep(pair.b).length; //~ ERROR: the view `pair` can only be read in place
    }

    function writtenElement(bytes memory data) public pure {
        /// @custom:solar-view
        (uint256[] memory words) = abi.decode(data, (uint256[]));
        words[0] = 1; //~ ERROR: the view `words` can only be read in place
    }

    function writtenField(bytes memory data) public pure {
        /// @custom:solar-view
        (Pair memory pair) = abi.decode(data, (Pair));
        pair.a = 2; //~ ERROR: the view `pair` can only be read in place
    }

    function writtenNested(bytes memory data) public pure {
        /// @custom:solar-view
        (uint256[][] memory rows) = abi.decode(data, (uint256[][]));
        rows[0][0] = 3; //~ ERROR: the view `rows` can only be read in place
    }

    function aliased(bytes memory data) public pure returns (uint256) {
        /// @custom:solar-view
        (uint256[] memory words) = abi.decode(data, (uint256[]));
        uint256[] memory other = words; //~ ERROR: the view `words` can only be read in place
        return other.length;
    }

    function encoded(bytes memory data) public pure returns (bytes memory) {
        /// @custom:solar-view
        (bytes[] memory items) = abi.decode(data, (bytes[]));
        return abi.encode(items); //~ ERROR: the view `items` can only be read in place
    }

    function writes(bytes memory data) public pure returns (uint256) {
        /// @custom:solar-view
        (bytes[] memory items) = abi.decode(data, (bytes[]));
        /// @custom:solar-view
        bytes memory first = items[0];
        data[64] = 0x01; //~ ERROR: this may change bytes that the view `items` still reads
        return first.length;
    }

    function fieldWrites(bytes memory data) public pure returns (uint256) {
        /// @custom:solar-view
        (Pair memory pair) = abi.decode(data, (Pair));
        data[0] = 0x01; //~ ERROR: this may change bytes that the view `pair` still reads
        return pair.b.length;
    }

    // After the last read the bytes may change, and a calldata view borrows
    // nothing.
    function afterReads(bytes memory data, bytes calldata input) public pure returns (uint256 n) {
        /// @custom:solar-view
        (uint256[] memory words) = abi.decode(data, (uint256[]));
        n = words[0];
        data[0] = 0x01;
        /// @custom:solar-view
        (bytes[] memory items) = abi.decode(input, (bytes[]));
        data[1] = 0x02;
        n += items[0].length;
    }
}
