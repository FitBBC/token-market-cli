import { createPkce } from '../src/auth/pkce.js';
import { createHash } from 'node:crypto';
import { expect, test } from 'vitest';

test('creates S256 PKCE pairs', () => {
  const pkce = createPkce();
  expect(pkce.codeChallengeMethod).toBe('S256');
  expect(pkce.codeVerifier).toMatch(/^[A-Za-z0-9_-]+$/);
  expect(pkce.codeChallenge).toBe(createHash('sha256').update(pkce.codeVerifier).digest('base64url').replaceAll('=', ''));
});
