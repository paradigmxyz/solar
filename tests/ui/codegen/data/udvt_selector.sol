//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=hashes,bin-runtime --pretty-json

type Wad is uint256;

contract UdvtSelector {
    function unwrapAndAdd(Wad x, uint256 y) external pure returns (uint256) {
        return Wad.unwrap(x) + y;
    }
}
