contract EvalOrder {
    uint256 public log_;
    uint256[4] a;
    mapping(uint256 => uint256) m;
    struct S { uint256 x; uint256 y; }
    S s;

    function tick(uint256 tag) internal returns (uint256) { log_ = log_ * 10 + tag; return tag; }
    function tickv(uint256 tag, uint256 v) internal returns (uint256) { log_ = log_ * 10 + tag; return v; }

    function args() external returns (uint256) { sum3(tick(1), tick(2), tick(3)); return log_; }
    function sum3(uint256 x, uint256 y, uint256 z) internal pure returns (uint256) { return x + y + z; }
    function binop() external returns (uint256) { uint256 r = tick(1) + tick(2) * tick(3); return log_ * 100 + r; }
    function cmpChain() external returns (uint256) { bool b = tick(1) < tick(2) && tick(3) > tick(4); return log_ * 10 + (b ? 1 : 0); }
    function shortCircuit() external returns (uint256) { bool b = tickv(1, 0) == 1 && tick(2) == 2; b; return log_; }
    function shortCircuitOr() external returns (uint256) { bool b = tickv(1, 1) == 1 || tick(2) == 2; b; return log_; }
    function indexAssign() external returns (uint256) { a[tick(1)] = tick(2); return log_ * 10 + a[1]; }
    function indexAssignIdx() external returns (uint256) { a[tickv(1, 2)] = tickv(2, 7); return log_ * 100 + a[2]; }
    function compound() external returns (uint256) { a[tickv(1, 1)] += tickv(2, 5); return log_ * 100 + a[1]; }
    function compoundMap() external returns (uint256) { m[tickv(1, 3)] += tickv(2, 5); return log_ * 100 + m[3]; }
    function nestedIndex() external returns (uint256) { a[a[tickv(1, 0)]] = tickv(2, 9); return log_ * 100 + a[0]; }
    function ternaryOrder() external returns (uint256) { uint256 r = tickv(1, 1) == 1 ? tick(2) : tick(3); return log_ * 10 + r; }
    function tupleRhs() external returns (uint256) { (uint256 p, uint256 q) = (tick(1), tick(2)); return log_ * 100 + p * 10 + q; }
    function tupleLhs() external returns (uint256) { (a[tickv(1, 0)], a[tickv(2, 1)]) = (tickv(3, 5), tickv(4, 6)); return log_ * 100 + a[0] * 10 + a[1]; }
    function structAssign() external returns (uint256) { s = S(tick(1), tick(2)); return log_ * 100 + s.x * 10 + s.y; }
    function structNamed() external returns (uint256) { s = S({y: tick(1), x: tick(2)}); return log_ * 100 + s.x * 10 + s.y; }
    function arrayLiteral() external returns (uint256) { uint256[3] memory t = [tick(1), tick(2), tick(3)]; return log_ * 1000 + t[0] * 100 + t[1] * 10 + t[2]; }
    function encodeArgs() external returns (bytes32) { bytes memory b = abi.encode(tick(1), tick(2)); return keccak256(abi.encodePacked(b, log_)); }
    function memberCall() external returns (uint256) { uint256[] memory arr = new uint256[](2); arr[tickv(1, 0)] = tickv(2, 4); return log_ * 10 + arr[0]; }
    function deleteOrder() external returns (uint256) { a[1] = 5; delete a[tickv(1, 1)]; return log_ * 10 + a[1]; }
    function incIndex() external returns (uint256) { a[tickv(1, 2)]++; return log_ * 10 + a[2]; }
    function chainedAssign() external returns (uint256) { uint256 x; uint256 y; x = y = tick(1); return log_ * 100 + x * 10 + y; }
    function assignInCondition() external returns (uint256) { uint256 x; if ((x = tick(1)) == 1) { tick(2); } return log_ * 10 + x; }
    function loopOrder() external returns (uint256) { for (uint256 i = tickv(1, 0); i < tickv(2, 2); i = i + tickv(3, 1)) { tick(4); } return log_; }
    function unaryOrder() external returns (uint256) { uint256 x = 1; uint256 r = x++ + tick(2) + x; return log_ * 100 + r; }
    function modifierOrder() external returns (uint256) { return withMod(tick(1)); }
    modifier logMod() { tick(9); _; tick(8); }
    function withMod(uint256 v) internal logMod returns (uint256) { tick(v); return log_; }
    function externalArgs() external returns (uint256) { this.recv(tick(1), tick(2)); return log_; }
    function recv(uint256, uint256) external pure {}
    function newArrLen() external returns (uint256) { uint256[] memory t = new uint256[](tickv(1, 2)); t[tickv(2, 1)] = tickv(3, 3); return log_ * 10 + t[1]; }
    function revertOrder() external returns (uint256) { try this.failing(tick(1)) { } catch { tick(2); } return log_; }
    function failing(uint256) external pure { revert(); }
    function bytesConcat() external returns (bytes memory) { return bytes.concat(bytes32(tick(1)), bytes32(tick(2)), bytes32(log_)); }
    function stringIndexWrite() external returns (bytes memory) { bytes memory b = new bytes(2); b[tickv(1, 0)] = bytes1(uint8(tickv(2, 7))); return abi.encodePacked(b, log_); }
    function mappingKeyOrder() external returns (uint256) { m[tickv(1, 1)] = m[tickv(2, 1)] + tickv(3, 4); return log_ * 100 + m[1]; }
    function pushOrder() external returns (uint256) { dyn.push(tick(1)); dyn.push(tick(2)); return log_ * 100 + dyn[0] * 10 + dyn[1]; }
    uint256[] dyn;
    function pushIdxOrder() external returns (uint256) { dyn.push(); dyn.push(); dyn[tickv(1, 1)] = tickv(2, 3); return log_ * 10 + dyn[1]; }
    function swapStorage() external returns (uint256) { a[0] = 1; a[1] = 2; (a[tickv(1, 0)], a[tickv(2, 1)]) = (a[tickv(3, 1)], a[tickv(4, 0)]); return log_ * 100 + a[0] * 10 + a[1]; }
    function storageRefOrder() external returns (uint256) { s.x = 1; s.y = 2; S storage r = pick(tick(1)); r.x = tick(2); return log_ * 100 + s.x * 10 + s.y; }
    function pick(uint256) internal view returns (S storage) { return s; }
}
