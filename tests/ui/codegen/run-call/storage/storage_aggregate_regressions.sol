//@ codegen-matrix: standard
//@ run-call: StorageAggregateRegressions::run => 1

contract StorageAggregateRegressions {
    struct Pair {
        uint256 left;
        uint256 right;
    }

    uint256[2][2] internal nested;
    Pair[] internal pairs;
    uint256[2][] internal fixedRows;
    uint256[][] internal rows;
    bytes[] internal blobs;
    uint24[] internal packed;
    uint256[] internal words;
    uint256[][] internal deletedRows;

    function run() external returns (uint256) {
        nested[0][0] = 1;
        nested[0][1] = 2;
        nested[1][0] = 3;
        nested[1][1] = 4;
        uint256[2][2] memory nestedCopy = nested;
        require(nestedCopy[0][1] == 2 && nestedCopy[1][0] == 3, "nested copy");

        pairs.push();
        pairs[0] = Pair({left: 11, right: 22});
        require(pairs[0].left == 11 && pairs[0].right == 22, "struct assign");

        fixedRows.push();
        uint256[2] memory fixedValue = [uint256(31), uint256(32)];
        fixedRows[0] = fixedValue;
        require(fixedRows[0][0] == 31 && fixedRows[0][1] == 32, "fixed assign");

        rows.push();
        uint256[] memory row = new uint256[](2);
        row[0] = 41;
        row[1] = 42;
        rows[0] = row;
        require(rows[0].length == 2 && rows[0][1] == 42, "dynamic assign");

        blobs.push();
        blobs[0] = hex"aabbcc";
        require(blobs[0].length == 3 && blobs[0][1] == 0xbb, "bytes assign");

        packed.push(7);
        packed.push(8);
        words.push(9);
        words.push(10);
        deletedRows.push();
        deletedRows[0].push(12);
        delete pairs;
        delete fixedRows;
        delete blobs;
        delete packed;
        delete words;
        delete deletedRows;
        assembly {
            sstore(pairs.slot, 1)
            sstore(fixedRows.slot, 1)
            sstore(blobs.slot, 1)
            sstore(packed.slot, 2)
            sstore(words.slot, 2)
            sstore(deletedRows.slot, 1)
        }
        require(pairs[0].left == 0 && pairs[0].right == 0, "struct delete");
        require(fixedRows[0][0] == 0 && fixedRows[0][1] == 0, "fixed delete");
        require(blobs[0].length == 0, "bytes delete");
        require(packed[0] == 0 && packed[1] == 0, "packed delete");
        require(words[0] == 0 && words[1] == 0, "word delete");
        require(deletedRows[0].length == 0, "nested delete");
        return 1;
    }
}
