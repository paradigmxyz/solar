# c1: certify the three controls on solar 92c59b66

Receipts only. Driver solar 92c59b665bbd7ce17b39dd2317e5e66379408528, harness f597a68ca10e760cb7dfe68f505f4e0d39309896
(PHASE=c1: certify only, no replay), solidity-lean 334d1d924f8f6c29ac827fd81d1cc739721b479a with a package cache fetched by
lake at the same commit (records' packages.cache_sha256 5df059807ee5f620fc453d17b47f4db8e2c4d690b77ca918195eabe38d69b8a2).
Cases: the three control rows of cases.tsv (c1.tsv inside run-meta.tgz).

Each control is certified, replay pending c2: its independent cold replay is a separate phase on another machine, from
the record in its archive. Nothing here closes a control.

- outbox/NAME.tgz with NAME.tgz.sha256 (digest taken where it ran, verified on copy): the case directory with its record.
- run-meta.tgz: run.log, table.md, producer.json/.log, admission.txt, run-manifest.txt, versions.txt, setup.log,
  run-scope.txt, c1.tsv, harness-checked-out.txt.
- receipts.sha256: every file here.
