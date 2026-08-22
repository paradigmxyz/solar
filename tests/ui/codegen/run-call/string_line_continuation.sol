//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: unicodeLine() => "😃, 😭, and 😈"
//@ run-call: asciiLine() => "a   b"
// ported-from: test/libsolidity/semanticTests/strings/unicode_string.sol

contract StringLineContinuation {
    function unicodeLine() external pure returns (string memory) {
        return unicode"😃, 😭,\
 and 😈";
    }

    function asciiLine() external pure returns (string memory) {
        return "a \
  b";
    }
}
