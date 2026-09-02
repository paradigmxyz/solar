import { readFile, readdir, rm } from 'node:fs/promises'
import { join, resolve } from 'node:path'

const root = resolve(process.argv[2] ?? '')
if (!process.argv[2] || !root.endsWith('/data')) throw new Error('Expected a data directory')
const index = JSON.parse(await readFile(join(root, 'index.json'), 'utf8'))
const keep = new Set(index.runs.map((run) => run.commit))
for (const commit of await readdir(join(root, 'runs'))) {
  if (!keep.has(commit) && /^[0-9a-f]{40}$/.test(commit)) {
    await rm(join(root, 'runs', commit), { recursive: true })
  }
}
