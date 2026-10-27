import { expect, test } from 'vitest';
import { resolveModel } from '../src/models/model-presets.js';

test('defaults to GLM 5.3', () => {
  expect(resolveModel()).toBe('glm-5.2');
});

test('resolves the fast model preset', () => {
  expect(resolveModel('fast')).toBe('deepseek-v4.1-flash');
});
