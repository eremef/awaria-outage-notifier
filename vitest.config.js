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
  },
})
