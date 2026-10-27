import { expect, test } from 'vitest';
import { adapters, getAdapter, toolId } from '../src/adapters/registry.js';

test('registers all first-phase tools', () => {
  expect(adapters.map((adapter) => adapter.id)).toEqual(['claude-code', 'codex', 'workbuddy', 'deepseek-harness', 'openclaw', 'opencode']);
});

test('rejects unsupported tools', () => {
  expect(() => toolId('unknown')).toThrow('unsupported tool: unknown');
});

test('finds adapter by id', () => {
  expect(getAdapter('claude-code').name).toBe('Claude Code');
});
