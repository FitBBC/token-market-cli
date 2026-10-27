#!/usr/bin/env node
import { Command } from 'commander';

const program = new Command();

program
  .name('tokensmarket')
  .description('Configure Token Market for popular AI agent tools')
  .version('0.1.0');

program
  .command('setup')
  .description('Authorize Token Market and configure an agent tool')
  .argument('[tool]', 'target tool id')
  .option('--model <preset>', 'recommended, fast, cheap, or powerful')
  .action(async (tool: string | undefined, options: { model?: string }) => {
    const { setup } = await import('./commands/setup.js');
    await setup(tool, options.model);
  });

program
  .command('doctor')
  .description('Check Token Market connectivity and local configuration')
  .argument('[tool]', 'target tool id')
  .action(async (tool?: string) => {
    const { doctor } = await import('./commands/doctor.js');
    await doctor(tool);
  });

program
  .command('test')
  .description('Run configured request tests')
  .argument('[tool]', 'target tool id')
  .option('--models', 'test model list')
  .option('--chat', 'test chat')
  .option('--stream', 'test streaming')
  .option('--tool-call', 'test tool call')
  .action(async (tool: string, options: Record<string, boolean | undefined>) => {
    const { test } = await import('./commands/test.js');
    const selected = Object.entries(options).filter(([, enabled]) => enabled).map(([name]) => name.replace('-call', '-call'));
    await test(tool, selected.length ? selected as any : ['models', 'chat', 'stream', 'tool-call']);
  });

program
  .command('keys')
  .description('List keys managed by this CLI')
  .action(async () => {
    const { keys } = await import('./commands/keys.js');
    await keys();
  });

program.parseAsync();
