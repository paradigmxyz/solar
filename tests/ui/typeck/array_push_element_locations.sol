// Storage-array `push(x)` takes its argument location-less: the value is
// copied into storage, so structs, strings, bytes, UDVTs, and nested arrays
// convert from any data location, matching solc's direct-storage-reference
// conversion rule. `push()` still returns a storage reference.

type Price is uint128;

contract C {
    struct S {
        uint256 a;
        string s;
    }

    struct T {
        bytes b;
    }

    S[] ss;
    string[] strs;
    bytes[] bs;
    Price[] prices;
    uint256[][] nested;
    uint256[2][] pairs;
    uint256[3][] triples;
    int256[2][] signedPairs;

    function ok(
        S memory m,
        S calldata c,
        string memory str,
        uint256[] memory arr,
        uint8[2] memory narrow
    ) public {
        ss.push(m);
        ss.push(c);
        ss.push(ss[0]);
        strs.push(str);
        strs.push("literal");
        bs.push(hex"aa");
        prices.push(Price.wrap(1));
        nested.push(arr);
        // A copy into storage converts element-wise, so an inline array literal
        // widens to the element type and may be shorter than it.
        pairs.push([1, 2]);
        pairs.push(narrow);
        triples.push([1, 2]);
        S storage r = ss.push();
        r.a = 1;
    }

    function bad(T memory t, uint256[] memory m) public {
        ss.push(5); //~ ERROR: no matching member `push` found on type `struct C.S[] storage`
        ss.push(t); //~ ERROR: no matching member `push` found on type `struct C.S[] storage`
        strs.push(true); //~ ERROR: no matching member `push` found on type `string[] storage`
        m.push(1); //~ ERROR: member `push` not found on type `uint256[] memory`
        pairs.push([1, 2, 3]); //~ ERROR: no matching member `push` found on type `uint256[2][] storage`
        signedPairs.push([1, 2]); //~ ERROR: no matching member `push` found on type `int256[2][] storage`
    }
}
