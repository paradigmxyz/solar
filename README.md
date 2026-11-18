# verify_token box script

`vt_box.sh MEMORY_MAX_BYTES [OUT_DIR]` runs verify_token.sol (contract Token, SPEC natspec)
through the generic certify driver at solar 085fbaa8 and solidity-lean 334d1d92, inside a
memory-limited systemd scope under a 2 h wall gate. The steps are the producer build, the source
and bytecode envelopes, the driver run (with its caller route recorded verbatim), the driver's
fresh replay of the record, and three negatives that must be refused. See the script header.

hsep (MapSlotsSeparated) is reported as an open assumption, and deployment as excluded.
