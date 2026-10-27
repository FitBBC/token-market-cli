import { randomBytes, createHash } from 'node:crypto';

export function createPkce(): { codeVerifier: string; codeChallenge: string; codeChallengeMethod: 'S256' } {
  const codeVerifier = base64Url(randomBytes(48));
  const codeChallenge = base64Url(createHash('sha256').update(codeVerifier).digest());

  return { codeVerifier, codeChallenge, codeChallengeMethod: 'S256' };
}

export function base64Url(input: Buffer): string {
  return input.toString('base64url').replaceAll('=', '');
}
