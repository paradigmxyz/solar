import { copyFile, mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { basename, join, resolve } from 'node:path'

const allowedFiles = new Map([
  ['input.json', ['Compiler input', 'json']],
  ['output.json', ['Compiler output', 'json']],
  ['mir.mir', ['MIR', 'text']],
  ['creation.evmir', ['Creation EVM IR', 'text']],
  ['runtime.evmir', ['Runtime EVM IR', 'text']],
  ['optimized-ir.yul', ['Optimized Yul IR', 'solidity']],
  ['creation.disasm', ['Creation disassembly', 'assembly']],
  ['runtime.disasm', ['Runtime disassembly', 'assembly']],
  ['creation.hex', ['Creation bytecode', 'text']],
  ['runtime.hex', ['Runtime bytecode', 'text']],
])

function args() {
  const values = Object.fromEntries(process.argv.slice(2).map((value, index, all) => value.startsWith('--') ? [value.slice(2), all[index + 1]] : []))
  for (const required of ['results', 'artifacts', 'output', 'commit']) {
    if (!values[required]) throw new Error(`Missing --${required}`)
  }
  return values
}

function metric(results, key) {
  const values = results.map((result) => result.compilers?.solar).filter((compiler) => compiler?.status === 'ok').map((compiler) => compiler[key])
  return values.length && values.every((value) => typeof value === 'number') ? values.reduce((sum, value) => sum + value, 0) : null
}

async function artifactManifest(source, destination, results) {
  const manifest = {}
  for (const result of results) {
    const benchmark = result.test_id
    if (!/^[\w.-]+$/.test(benchmark)) throw new Error(`Unsafe benchmark ID: ${benchmark}`)
    const files = new Map()
    for (const compiler of ['solar', 'solc']) {
      const directory = join(source, benchmark, compiler)
      let names = []
      try { names = await readdir(directory) } catch { continue }
      for (const name of names) {
        const metadata = allowedFiles.get(name)
        if (!metadata) continue
        const sourceFile = join(directory, name)
        const size = (await stat(sourceFile)).size
        if (size > 32 * 1024 * 1024) throw new Error(`${benchmark}/${compiler}/${name} exceeds 32 MiB`)
        const target = join(destination, benchmark, compiler, name)
        await mkdir(resolve(target, '..'), { recursive: true })
        await copyFile(sourceFile, target)
        const entry = files.get(name) ?? { path: name, label: metadata[0], language: metadata[1], bytes: 0, compilers: [] }
        entry.bytes = Math.max(entry.bytes, size)
        entry.compilers.push(compiler)
        files.set(name, entry)
      }
    }
    if (files.size) manifest[benchmark] = [...files.values()]
  }
  return manifest
}

const options = args()
const document = JSON.parse(await readFile(options.results, 'utf8'))
const results = document.results ?? document
if (!Array.isArray(results)) throw new Error('Benchmark results must be an array')
const output = resolve(options.output)
const runDirectory = join(output, 'runs', options.commit)
await mkdir(runDirectory, { recursive: true })
const artifacts = await artifactManifest(resolve(options.artifacts), runDirectory, results)
const timestamp = options.timestamp ?? new Date().toISOString()
const run = {
  schemaVersion: 1,
  commit: options.commit,
  branch: options.branch || null,
  pr: options.pr ? Number(options.pr) : null,
  timestamp,
  results,
  artifacts,
}
await writeFile(join(runDirectory, 'run.json'), `${JSON.stringify(run)}\n`)

const indexPath = join(output, 'index.json')
let index = { schemaVersion: 1, updatedAt: null, runs: [] }
try { index = JSON.parse(await readFile(indexPath, 'utf8')) } catch {}
const summary = {
  commit: options.commit,
  timestamp,
  branch: options.branch || null,
  pr: options.pr ? Number(options.pr) : null,
  benchmarkCount: results.length,
  metrics: {
    compileTime: metric(results, 'compile_time_seconds'),
    creationSize: metric(results, 'bytecode_size'),
    runtimeSize: metric(results, 'runtime_size'),
    deployGas: metric(results, 'deploy_gas'),
    runtimeGas: metric(results, 'total_gas'),
  },
}
index.runs = [summary, ...index.runs.filter((run) => run.commit !== options.commit)].sort((a, b) => b.timestamp.localeCompare(a.timestamp)).slice(0, 200)
index.updatedAt = timestamp
await mkdir(output, { recursive: true })
await writeFile(indexPath, `${JSON.stringify(index)}\n`)
console.log(`Imported ${basename(options.results)} for ${options.commit.slice(0, 8)}`)
