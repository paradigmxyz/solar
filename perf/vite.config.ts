import { defineConfig, lazyPlugins } from 'vite-plus'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  base: '/solar/',
  plugins: lazyPlugins(() => [react(), tailwindcss()]),
  build: {
    target: 'es2022',
  },
  fmt: {
    ignorePatterns: ['dist/**', 'public/data/**'],
    semi: false,
    singleQuote: true,
  },
  lint: {
    ignorePatterns: ['dist', 'public/data'],
    options: {
      typeAware: true,
      typeCheck: true,
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
})
