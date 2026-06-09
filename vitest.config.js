import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'jsdom',
    pool: 'forks',
    singleFork: true,
    // Keep reasonable timeouts
    testTimeout: 30000,
    hookTimeout: 30000,
    teardownTimeout: 30000,
    exclude: ['**/node_modules/**', '**/dist/**', '**/cypress/**', '**/.{idea,git,cache,output,temp}/**', '**/{karma,rollup,webpack,vite,vitest,jest,ava,babel,nyc,cypress,tsup,build,eslint,prettier}.config.*', 'tests/a11y.spec.js'],
  },
})
