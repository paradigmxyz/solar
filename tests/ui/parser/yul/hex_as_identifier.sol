// ported-from: test/libsolidity/syntaxTests/string/hex_as_identifier.sol

function g() pure {
    assembly {
        let hex := 1 //~ ERROR: expected identifier
    }
}
