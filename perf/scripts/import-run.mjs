import { lstat, mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { basename, dirname, join, resolve } from 'node:path'

const allowedFiles = new Map([
  ['input.json', ['Compiler input', 'json', '0.json']],
  ['output.json', ['Compiler output', 'json', '1.json']],
  ['mir.mir', ['MIR', 'text', '2.json']],
  ['creation.evmir', ['Creation EVM IR', 'text', '3.json']],
  ['runtime.evmir', ['Runtime EVM IR', 'text', '4.json']],
  ['optimized-ir.yul', ['Optimized Yul IR', 'solidity', '5.json']],
  ['creation.disasm', ['Creation disassembly', 'asm', '6.json']],
  ['runtime.disasm', ['Runtime disassembly', 'asm', '7.json']],
  ['creation.hex', ['Creation bytecode', 'text', '8.json']],
  ['runtime.hex', ['Runtime bytecode', 'text', '9.json']],
])

function args() {
  const values = Object.create(null)
  for (let index = 2; index < process.argv.length; index += 2) {
    const option = process.argv[index]
    const value = process.argv[index + 1]
    if (!option.startsWith('--') || value === undefined) throw new Error(`Invalid argument: ${option}`)
    values[option.slice(2)] = value
  }
  for (const required of ['results', 'artifacts', 'output', 'commit']) {
    if (!values[required]) throw new Error(`Missing --${required}`)
  }
  return values
}

function metric(results, key) {
  const values = results
    .map((result) => result.compilers?.solar)
    .filter((compiler) => compiler?.status === 'ok' && typeof compiler[key] === 'number')
    .map((compiler) => compiler[key])
  return values.length ? values.reduce((sum, value) => sum + value, 0) : null
}

async function artifactManifest(source, destination, results) {
  const manifest = Object.create(null)
  let totalSize = 0
  for (const result of results) {
    const benchmark = result.test_id
    if (!/^[\w.-]+$/.test(benchmark) || benchmark === '.' || benchmark === '..') throw new Error(`Unsafe benchmark ID: ${benchmark}`)
    const files = new Map()
    for (const compiler of ['solar', 'solc']) {
      const directory = join(source, benchmark, compiler)
      let names = []
      try { names = await readdir(directory) } catch { continue }
      for (const name of names) {
        const metadata = allowedFiles.get(name)
        if (!metadata) continue
        const sourceFile = join(directory, name)
        const file = await lstat(sourceFile)
        if (!file.isFile()) throw new Error(`Artifact is not a regular file: ${benchmark}/${compiler}/${name}`)
        const size = file.size
        if (size > 32 * 1024 * 1024) throw new Error(`${benchmark}/${compiler}/${name} exceeds 32 MiB`)
        totalSize += size
        if (totalSize > 256 * 1024 * 1024) throw new Error('Artifact run exceeds 256 MiB')
        const target = join(destination, benchmark, compiler, metadata[2])
        await mkdir(dirname(target), { recursive: true })
        const storagePath = metadata[2]
        await writeFile(target, `${JSON.stringify(await readFile(sourceFile, 'utf8'))}\n`)
        const entry = files.get(name) ?? { path: name, storagePath, label: metadata[0], language: metadata[1], bytes: 0, compilers: [] }
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
if (!/^[0-9a-f]{40}$/.test(options.commit)) throw new Error('Commit must be a full SHA')
if ((await stat(options.results)).size > 32 * 1024 * 1024) throw new Error('Results exceed 32 MiB')
const document = JSON.parse(await readFile(options.results, 'utf8'))
const results = document.results ?? document
if (!Array.isArray(results)) throw new Error('Benchmark results must be an array')
if (results.length > 500) throw new Error('Benchmark results exceed 500 entries')
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
