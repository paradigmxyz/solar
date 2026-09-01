# Solar performance site

This standalone Vite app reads static benchmark data from the `gh-pages` branch. Local development
uses `https://raw.githubusercontent.com/paradigmxyz/solar/gh-pages/data/` and falls back to
`public/data` when the branch has not been published. Set `VITE_PERF_DATA_URL` to override the data
root. The homepage loads only `index.json`; commit results and compiler artifacts load when a
comparison opens.

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
