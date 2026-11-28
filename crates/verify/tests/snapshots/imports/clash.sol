pragma solidity ^0.8.20;

// `IERC20` reaches this file from two declarations, one through `ViaA.sol`: solc rejects it.
import "./auxiliary/ViaA.sol";
import {IERC20} from "./auxiliary/TokenB.sol";

contract Clash {}
