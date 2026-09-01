# Solar performance site

This standalone Vite app reads static benchmark data from `public/data`. The homepage loads only
`index.json`; commit results and compiler artifacts load when a comparison opens.

```bash
npm ci
npm run dev
```

Import a local benchmark run before starting the site:

```bash
node scripts/import-run.mjs \
  --results ../target/codegen-bench/results.json \
  --artifacts ../target/codegen-bench/artifacts \
  --output public/data \
  --commit "$(git rev-parse HEAD)" \
  --branch "$(git branch --show-current)"
```

Do not commit imported run data. CI keeps it on the `gh-pages` branch and deploys that branch with
GitHub Pages.
