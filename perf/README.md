# Solar performance site

This standalone Vite+ app reads static benchmark data from the `gh-pages` branch. Local development
uses `https://raw.githubusercontent.com/paradigmxyz/solar/gh-pages/data/` and falls back to
`public/data` when the branch has not been published. Set `VITE_PERF_DATA_URL` to override the data
root. The homepage loads only `index.json`; commit results and compiler artifacts load when a
comparison opens.

```bash
pnpm install --frozen-lockfile
pnpm dev
```

Run `pnpm check`, `pnpm test`, and `pnpm build` before changing the site.

## API worker

`src/worker.ts` is a Hono Cloudflare Worker that proxies the same immutable `gh-pages` run archive.
It only permits the published index, run documents, and numbered artifact files. This keeps the
static GitHub Pages deployment working while allowing a Worker deployment to add caching and keep a
GitHub token off the client. Start it with `pnpm worker:dev`, then point the site at it with:

```bash
VITE_PERF_API_URL=http://127.0.0.1:8787 pnpm dev
```

Set `GITHUB_TOKEN` in an untracked `.dev.vars` file when GitHub's unauthenticated rate limit is too
low. `GITHUB_REPOSITORY` and `PERF_DATA_REF` default to `paradigmxyz/solar` and `gh-pages`.

The Worker does not query ClickHouse yet. Add that source after we choose a run and artifact schema;
the current benchmark publisher has no ClickHouse table to query.

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
