// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    address owner;
    // expect: conversion of a non-literal not exported
    /// @custom:ensures uint160(owner) != 0
    function f() external {}
}
