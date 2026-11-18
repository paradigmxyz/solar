# c1b: certify the three controls on solar 65d87ab3

Receipts only. Driver solar 65d87ab35420acd048ee1015113b6a887ab065ba (build provenance version 2), harness
8d4bae65a179149c2e05c002399072dc9de72403 (PHASE=c1: certify only, no replay), solidity-lean
334d1d924f8f6c29ac827fd81d1cc739721b479a with a package cache fetched by lake at the same commit (records'
packages.cache_sha256 5df059807ee5f620fc453d17b47f4db8e2c4d690b77ca918195eabe38d69b8a2). Solar was built from a fresh
clone into an empty target. Cases: the three control rows of cases.tsv (c1.tsv in run-meta.tgz).

Each control is certified, replay pending c2. Nothing here closes a control.

- outbox/NAME.tgz with NAME.tgz.sha256 (digest taken where it ran, verified on copy): the case directory with its
  record, the exact certify command (command), certify.log, status, scope.txt and producer-check.json.
- run-meta.tgz: run.log, table.md, producer.json/.log, admission.txt, run-manifest.txt (with the full run command),
  versions.txt, setup.log, run-scope.txt, c1.tsv, harness-checked-out.txt, go.log.
- receipts.sha256: every file here.
