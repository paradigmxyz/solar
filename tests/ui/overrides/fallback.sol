contract VirtualFallback {
    fallback() external virtual {}
}

contract MissingOverride is VirtualFallback {
    fallback() external {} //~ ERROR: overriding function is missing `override` specifier
}

contract ConcreteFallback {
    fallback() external {} //~ ERROR: cannot override non-virtual function
}

contract NonVirtualOverride is ConcreteFallback {
    fallback() external override {}
}
