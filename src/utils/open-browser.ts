import { spawn } from 'node:child_process';

export async function open(url: string): Promise<void> {
  const platform = process.platform;
  const command = platform === 'darwin' ? 'open' : platform === 'win32' ? 'start' : 'xdg-open';
  const args = platform === 'win32' ? ['', url] : [url];

  await new Promise<void>((resolveFunction, rejectFunction) => {
    const child = spawn(command, args, { detached: true, stdio: 'ignore', shell: platform === 'win32' });
    child.once('error', rejectFunction);
    child.once('spawn', () => {
      child.unref();
      resolveFunction();
    });
  });
}
