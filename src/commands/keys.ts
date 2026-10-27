import { listManagedKeys } from '../config/managed-keys.js';

export async function keys(): Promise<void> {
  const records = await listManagedKeys();
  if (!records.length) {
    console.log('No keys have been configured yet.');
    return;
  }

  for (const record of records) {
    console.log(`Tool: ${record.tool}`);
    console.log(`Key name: ${record.keyName}`);
    console.log(`Key id: ${record.keyId}`);
    console.log(`Created: ${record.createdAt}`);
    console.log('Revocable: yes (from the Token Market dashboard)');
    console.log('');
  }
}
